package app

import "github.com/tinymins/sempre/internal/layout"

type commandRegistrar interface {
	Register(layout.Layout) (func() error, error)
	Unregister(layout.Layout) error
	Check(layout.Layout) error
}

type platformCommandRegistrar struct{}

func (platformCommandRegistrar) Register(paths layout.Layout) (func() error, error) {
	return registerCommand(paths)
}

func (platformCommandRegistrar) Unregister(paths layout.Layout) error {
	return unregisterCommand(paths)
}

func (platformCommandRegistrar) Check(paths layout.Layout) error {
	return checkCommandRegistration(paths)
}
