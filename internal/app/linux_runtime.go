package app

import (
	"context"
	"net/http"

	"github.com/tinymins/sempre/internal/transparentproxy"
)

func (manager *Manager) LinuxNetworkInventory(ctx context.Context) (transparentproxy.Inventory, error) {
	return manager.transparent.Inventory(ctx)
}

func (admin *adminServer) systemNetwork(writer http.ResponseWriter, request *http.Request) {
	inventory, err := admin.manager.LinuxNetworkInventory(request.Context())
	if err != nil {
		admin.internalError(writer, err)
		return
	}
	apiWriteJSON(writer, http.StatusOK, inventory)
}
