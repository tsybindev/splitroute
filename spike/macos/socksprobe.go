// socksprobe — минимальный SOCKS5-клиент для проверки process-правил (S1-T4, S1-T9).
//
// Зачем: системный /usr/bin/curl на macOS — restricted platform binary, его копии
// в $TMPDIR убиваются AMFI (SIGKILL). Свой Go-бинарь можно копировать под разными
// именами и путями, что и нужно для проверки process_name / process_path_regex.
//
// Использование:
//
//	socksprobe -proxy 127.0.0.1:10808 -url http://192.0.2.10/        # TCP
//	socksprobe -udp -proxy 127.0.0.1:10808 -url http://192.0.2.10:9999/  # UDP ASSOCIATE
//
// Печатает тело ответа в stdout. Код 0 — успех, 2 — ошибка SOCKS5.
package main

import (
	"encoding/binary"
	"flag"
	"fmt"
	"io"
	"net"
	"net/url"
	"os"
	"strconv"
	"strings"
	"time"
)

func main() {
	proxyAddr := flag.String("proxy", "", "адрес SOCKS5-прокси host:port")
	targetURL := flag.String("url", "", "URL; для TCP — GET, для UDP — адрес назначения")
	udpMode := flag.Bool("udp", false, "использовать SOCKS5 UDP ASSOCIATE")
	flag.Parse()

	host, port := parseTarget(*targetURL)
	if *udpMode {
		runUDP(*proxyAddr, host, port)
		return
	}
	runTCP(*proxyAddr, host, port)
}

func parseTarget(raw string) (string, int) {
	parsed, err := url.Parse(raw)
	if err != nil {
		fail("невалидный URL: %v", err)
	}
	port := parsed.Port()
	if port == "" {
		port = "80"
	}
	number, err := strconv.Atoi(port)
	if err != nil {
		fail("невалидный порт: %v", err)
	}
	return parsed.Hostname(), number
}

// handshake выполняет приветствие и CONNECT/UDP ASSOCIATE, возвращает соединение
// и адрес relay для UDP (для CONNECT — пустой).
func handshake(proxyAddr, host string, port int, command byte) (net.Conn, string, int) {
	conn, err := net.Dial("tcp", proxyAddr)
	if err != nil {
		fail("не удалось подключиться к прокси: %v", err)
	}
	if _, err := conn.Write([]byte{0x05, 0x01, 0x00}); err != nil {
		fail("ошибка приветствия: %v", err)
	}
	greeting := make([]byte, 2)
	if _, err := io.ReadFull(conn, greeting); err != nil {
		fail("ошибка чтения приветствия: %v", err)
	}
	if greeting[0] != 0x05 || greeting[1] != 0x00 {
		fail("прокси не принял метод без аутентификации")
	}

	request := []byte{0x05, command, 0x00}
	request = appendAddress(request, host, port)
	if _, err := conn.Write(request); err != nil {
		fail("ошибка запроса: %v", err)
	}

	header := make([]byte, 4)
	if _, err := io.ReadFull(conn, header); err != nil {
		fail("ошибка чтения ответа: %v", err)
	}
	if header[1] != 0x00 {
		fail("SOCKS5 отказал, код %d", header[1])
	}
	relayHost, relayPort := readAddress(conn, header[3])
	return conn, relayHost, relayPort
}

func appendAddress(buffer []byte, host string, port int) []byte {
	if ip := net.ParseIP(host).To4(); ip != nil {
		buffer = append(buffer, 0x01)
		buffer = append(buffer, ip...)
	} else {
		buffer = append(buffer, 0x03, byte(len(host)))
		buffer = append(buffer, host...)
	}
	return binary.BigEndian.AppendUint16(buffer, uint16(port))
}

func readAddress(conn net.Conn, atyp byte) (string, int) {
	switch atyp {
	case 0x01:
		address := make([]byte, 4)
		io.ReadFull(conn, address)
		port := make([]byte, 2)
		io.ReadFull(conn, port)
		return net.IP(address).String(), int(binary.BigEndian.Uint16(port))
	case 0x03:
		length := make([]byte, 1)
		io.ReadFull(conn, length)
		address := make([]byte, int(length[0]))
		io.ReadFull(conn, address)
		port := make([]byte, 2)
		io.ReadFull(conn, port)
		return string(address), int(binary.BigEndian.Uint16(port))
	case 0x04:
		address := make([]byte, 16)
		io.ReadFull(conn, address)
		port := make([]byte, 2)
		io.ReadFull(conn, port)
		return net.IP(address).String(), int(binary.BigEndian.Uint16(port))
	}
	fail("неизвестный ATYP в ответе: %d", atyp)
	return "", 0
}

func runTCP(proxyAddr, host string, port int) {
	conn, _, _ := handshake(proxyAddr, host, port, 0x01)
	defer conn.Close()

	fmt.Fprintf(conn, "GET / HTTP/1.1\r\nHost: %s\r\nConnection: close\r\n\r\n", host)
	response, err := io.ReadAll(conn)
	if err != nil {
		fail("ошибка чтения ответа: %v", err)
	}
	text := string(response)
	if index := strings.Index(text, "\r\n\r\n"); index >= 0 {
		fmt.Print(text[index+4:])
	} else {
		fmt.Print(text)
	}
}

func runUDP(proxyAddr, host string, port int) {
	control, relayHost, relayPort := handshake(proxyAddr, "0.0.0.0", 0, 0x03)
	defer control.Close()
	if relayHost == "0.0.0.0" || relayHost == "" {
		relayHost = strings.Split(proxyAddr, ":")[0]
	}

	remote, err := net.ResolveUDPAddr("udp", net.JoinHostPort(relayHost, strconv.Itoa(relayPort)))
	if err != nil {
		fail("не удалось разобрать relay-адрес: %v", err)
	}
	conn, err := net.DialUDP("udp", nil, remote)
	if err != nil {
		fail("не удалось открыть UDP: %v", err)
	}
	defer conn.Close()

	payload := []byte("splitroute-udp-probe")
	packet := []byte{0x00, 0x00, 0x00} // RSV, RSV, FRAG
	packet = appendAddress(packet, host, port)
	packet = append(packet, payload...)
	if _, err := conn.Write(packet); err != nil {
		fail("ошибка отправки UDP: %v", err)
	}

	_ = conn.SetReadDeadline(time.Now().Add(8 * time.Second))
	buffer := make([]byte, 65535)
	count, err := conn.Read(buffer)
	if err != nil {
		fail("ошибка чтения UDP: %v", err)
	}
	// Пропускаем заголовок SOCKS5: 3 байта (RSV, RSV, FRAG) + ATYP + адрес + порт.
	offset := 4
	switch buffer[3] {
	case 0x01:
		offset += 4 + 2
	case 0x03:
		offset += 1 + int(buffer[4]) + 2
	case 0x04:
		offset += 16 + 2
	}
	fmt.Print(string(buffer[offset:count]))
}

func fail(format string, args ...any) {
	fmt.Fprintf(os.Stderr, format+"\n", args...)
	os.Exit(2)
}
