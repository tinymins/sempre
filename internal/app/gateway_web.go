package app

import (
	"net/http"

	"github.com/tinymins/sempre/internal/gateway"
	subscriptions "github.com/tinymins/sempre/internal/subscription"
)

func (manager *Manager) GatewayStatus(request *http.Request) (gateway.Status, error) {
	var transparent any
	catalog, err := manager.subscriptions.Read()
	if err == nil {
		document, readErr := manager.store.Read()
		if readErr == nil {
			if profile, findErr := subscriptions.FindProfile(&catalog, document.ActiveProfileID); findErr == nil {
				transparent = profile.TransparentProxy
			}
		}
	}
	return manager.gateway.Status(request.Context(), transparent)
}

func (admin *adminServer) gatewayGet(writer http.ResponseWriter, request *http.Request) {
	status, err := admin.manager.GatewayStatus(request)
	if err != nil {
		admin.internalError(writer, err)
		return
	}
	apiWriteJSON(writer, http.StatusOK, status)
}

func (admin *adminServer) gatewayPut(writer http.ResponseWriter, request *http.Request) {
	var config gateway.Config
	if !admin.decode(writer, request, &config) {
		return
	}
	saved, err := admin.manager.gateway.Update(config)
	if err != nil {
		admin.operationError(writer, err)
		return
	}
	reloaded, reloadErr := admin.manager.RequestReloadIfRunning()
	if reloadErr != nil {
		admin.operationError(writer, reloadErr)
		return
	}
	apiWriteJSON(writer, http.StatusOK, map[string]any{"config": saved, "reload_requested": reloaded})
}

func (admin *adminServer) gatewayValidate(writer http.ResponseWriter, request *http.Request) {
	var config gateway.Config
	if !admin.decode(writer, request, &config) {
		return
	}
	messages := gateway.ValidationMessages(config)
	apiWriteJSON(writer, http.StatusOK, map[string]any{"valid": len(messages) == 0, "errors": messages})
}

func (admin *adminServer) gatewayHostPlan(writer http.ResponseWriter, request *http.Request) {
	var input gateway.HostPlanRequest
	if !admin.decode(writer, request, &input) {
		return
	}
	plan, err := gateway.BuildHostPlan(input.Config)
	if err != nil {
		admin.operationError(writer, err)
		return
	}
	apiWriteJSON(writer, http.StatusOK, plan)
}

func (admin *adminServer) gatewayHostApply(writer http.ResponseWriter, request *http.Request) {
	var input gateway.HostApplyRequest
	if !admin.decode(writer, request, &input) {
		return
	}
	plan, err := gateway.ApplyHostPlan(request.Context(), input)
	if err != nil {
		_ = plan
		apiWriteError(writer, http.StatusConflict, "GATEWAY_HOST_APPLY_FAILED", err.Error(), nil)
		return
	}
	apiWriteJSON(writer, http.StatusOK, plan)
}

func (admin *adminServer) gatewayDNSQuery(writer http.ResponseWriter, request *http.Request) {
	var input struct {
		Name string `json:"name"`
		Type string `json:"type"`
	}
	if !admin.decode(writer, request, &input) {
		return
	}
	result, err := admin.manager.gateway.QueryDNS(request.Context(), input.Name, input.Type)
	if err != nil {
		admin.operationError(writer, err)
		return
	}
	apiWriteJSON(writer, http.StatusOK, result)
}

func (admin *adminServer) gatewayLeaseRevoke(writer http.ResponseWriter, request *http.Request) {
	var input struct {
		MAC string `json:"mac"`
	}
	if !admin.decode(writer, request, &input) {
		return
	}
	if err := admin.manager.gateway.RevokeLease(input.MAC); err != nil {
		admin.operationError(writer, err)
		return
	}
	apiWriteJSON(writer, http.StatusOK, map[string]bool{"changed": true})
}
