// socksprobe — минимальный SOCKS5-клиент для проверки process-правил (S1-T4).
//
// Зачем: системный /usr/bin/curl на macOS — restricted platform binary, его копии
// в $TMPDIR убиваются AMFI (SIGKILL). Свой Go-бинарь можно копировать под разными
// именами и путями, что и нужно для проверки process_name / process_path_regex.
//
// Использование:
//
//	socksprobe -proxy 127.0.0.1:10808 -url http://192.0.2.10/
//
// Печатает тело HTTP-ответа в stdout. Код 0 — успех, 2 — ошибка SOCKS5.
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
)

func main() {
	proxyAddr := flag.String("proxy", "", "адрес SOCKS5-прокси host:port")
	targetURL := flag.String("url", "", "URL для GET-запроса")
	flag.Parse()

	parsed, err := url.Parse(*targetURL)
	if err != nil {
		fail("невалидный URL: %v", err)
	}
	host := parsed.Hostname()
	port := parsed.Port()
	if port == "" {
		port = "80"
	}
	portNumber, err := strconv.Atoi(port)
	if err != nil {
		fail("невалидный порт: %v", err)
	}

	conn, err := net.Dial("tcp", *proxyAddr)
	if err != nil {
		fail("не удалось подключиться к прокси: %v", err)
	}
	defer conn.Close()

	// Приветствие: версия 5, один метод — без аутентификации.
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

	// CONNECT.
	request := []byte{0x05, 0x01, 0x00}
	if ip := net.ParseIP(host).To4(); ip != nil {
		request = append(request, 0x01)
		request = append(request, ip...)
	} else {
		request = append(request, 0x03, byte(len(host)))
		request = append(request, host...)
	}
	request = binary.BigEndian.AppendUint16(request, uint16(portNumber))
	if _, err := conn.Write(request); err != nil {
		fail("ошибка CONNECT: %v", err)
	}

	reply := make([]byte, 4)
	if _, err := io.ReadFull(conn, reply); err != nil {
		fail("ошибка чтения ответа CONNECT: %v", err)
	}
	if reply[1] != 0x00 {
		fail("SOCKS5 отказал в соединении, код %d", reply[1])
	}
	switch reply[3] {
	case 0x01:
		io.ReadFull(conn, make([]byte, 4+2))
	case 0x03:
		length := make([]byte, 1)
		io.ReadFull(conn, length)
		io.ReadFull(conn, make([]byte, int(length[0])+2))
	case 0x04:
		io.ReadFull(conn, make([]byte, 16+2))
	}

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

func fail(format string, args ...any) {
	fmt.Fprintf(os.Stderr, format+"\n", args...)
	os.Exit(2)
}
