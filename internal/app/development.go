package app

import (
	"context"
	"errors"
	"io"

	"github.com/tinymins/sempre/internal/layout"
	"github.com/tinymins/sempre/internal/service"
)

var errDevelopmentServiceUnavailable = errors.New("system service operations are unavailable in development mode")

type developmentService struct{}

func NewDevelopment(paths layout.Layout, output, errorOutput io.Writer) (*Manager, error) {
	return newManager(paths, output, errorOutput, developmentService{})
}

func (developmentService) Install(context.Context, string, string) error {
	return errDevelopmentServiceUnavailable
}

func (developmentService) Uninstall(context.Context) error { return errDevelopmentServiceUnavailable }
func (developmentService) Start(context.Context) error     { return errDevelopmentServiceUnavailable }
func (developmentService) Stop(context.Context) error      { return errDevelopmentServiceUnavailable }
func (developmentService) Restart(context.Context) error   { return errDevelopmentServiceUnavailable }

func (developmentService) Status(context.Context) (service.State, error) {
	return service.NotInstalled, nil
}

func (developmentService) Run(ctx context.Context, daemon func(context.Context) error) error {
	return daemon(ctx)
}
