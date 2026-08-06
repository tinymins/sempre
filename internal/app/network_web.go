package app

import (
	"net/http"
)

func (admin *adminServer) networkTest(writer http.ResponseWriter, request *http.Request) {
	apiWriteJSON(writer, http.StatusOK, admin.manager.NetworkTest(request.Context()))
}
