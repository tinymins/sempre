package app

import (
	"net/http"
	"strings"

	"github.com/tinymins/sempre/internal/tunnel"
)

func (admin *adminServer) tunnelsGet(writer http.ResponseWriter, _ *http.Request) {
	status, err := admin.manager.tunnels.Status()
	if err != nil {
		admin.internalError(writer, err)
		return
	}
	apiWriteJSON(writer, http.StatusOK, status)
}

func (admin *adminServer) tunnelsPut(writer http.ResponseWriter, request *http.Request) {
	var config tunnel.Config
	if !admin.decode(writer, request, &config) {
		return
	}
	saved, restart, err := admin.manager.UpdateTunnels(request.Context(), config)
	if err != nil {
		admin.operationError(writer, err)
		return
	}
	status, err := admin.manager.tunnels.Status()
	if err != nil {
		admin.internalError(writer, err)
		return
	}
	status.Config = saved
	apiWriteJSON(writer, http.StatusOK, map[string]any{"status": status, "core_restart_requested": restart})
}

func (admin *adminServer) tunnelInstall(writer http.ResponseWriter, request *http.Request) {
	status, err := admin.manager.tunnels.Install(request.Context())
	if err != nil {
		admin.operationError(writer, err)
		return
	}
	apiWriteJSON(writer, http.StatusOK, status)
}

func (admin *adminServer) tunnelAction(writer http.ResponseWriter, request *http.Request) {
	id := request.PathValue("id")
	action := request.PathValue("action")
	status, err := admin.manager.tunnels.Action(id, action)
	if err != nil {
		admin.operationError(writer, err)
		return
	}
	apiWriteJSON(writer, http.StatusAccepted, map[string]any{"action": action, "status": status})
}

func (admin *adminServer) tunnelLog(writer http.ResponseWriter, request *http.Request) {
	content, err := admin.manager.tunnels.Log(request.PathValue("id"))
	if err != nil {
		admin.operationError(writer, err)
		return
	}
	apiWriteJSON(writer, http.StatusOK, map[string]string{"content": strings.TrimSpace(content)})
}
