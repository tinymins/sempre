package core

import (
	"crypto/rand"
	"encoding/hex"
	"fmt"
	"net"
)

func NewPrivateControl(coreID, protocol string) (ControlSpec, error) {
	listener, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		return ControlSpec{}, fmt.Errorf("reserve internal core API address: %w", err)
	}
	address := listener.Addr().String()
	if err := listener.Close(); err != nil {
		return ControlSpec{}, err
	}
	secretBytes := make([]byte, 32)
	if _, err := rand.Read(secretBytes); err != nil {
		return ControlSpec{}, fmt.Errorf("generate internal core API secret: %w", err)
	}
	return ControlSpec{
		Core:     coreID,
		Protocol: protocol,
		BaseURL:  "http://" + address,
		Secret:   hex.EncodeToString(secretBytes),
	}, nil
}
