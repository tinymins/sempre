package main

import (
	"github.com/tinymins/sempre/internal/core"
	"github.com/tinymins/sempre/internal/core/mihomo"
	"github.com/tinymins/sempre/internal/core/singbox"
	"github.com/tinymins/sempre/internal/core/v2ray"
	"github.com/tinymins/sempre/internal/core/xray"
)

const (
	bundledSingBoxV11 = "1.11.15"
	bundledSingBoxV12 = "1.12.20"
	bundledSingBoxV13 = "1.13.18"
	bundledSingBoxV14 = "1.14.0-beta.13"
)

type releaseCoreRequest struct {
	Adapter   core.Adapter
	Reference string
	Channel   string
}

func releaseCoreRequests() []releaseCoreRequest {
	singBox := singbox.New()
	return []releaseCoreRequest{
		{Adapter: singBox, Reference: bundledSingBoxV11},
		{Adapter: singBox, Reference: bundledSingBoxV12},
		{Adapter: singBox, Reference: bundledSingBoxV13, Channel: core.Stable},
		{Adapter: singBox, Reference: bundledSingBoxV14},
		{Adapter: mihomo.New(), Reference: core.Stable, Channel: core.Stable},
		{Adapter: xray.New(), Reference: core.Stable, Channel: core.Stable},
		{Adapter: v2ray.New(), Reference: core.Stable, Channel: core.Stable},
	}
}
