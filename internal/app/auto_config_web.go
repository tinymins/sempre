package app

import "net/http"

func (admin *adminServer) autoConfigDiagnose(writer http.ResponseWriter, _ *http.Request) {
	report, err := admin.manager.DiagnoseCoreConfiguration()
	if err != nil {
		admin.operationError(writer, err)
		return
	}
	apiWriteJSON(writer, http.StatusOK, report)
}

func (admin *adminServer) autoConfigApply(writer http.ResponseWriter, request *http.Request) {
	var input struct {
		CandidateID string `json:"candidate_id"`
	}
	if !admin.decode(writer, request, &input) {
		return
	}
	result, err := admin.manager.ApplyCoreConfiguration(request.Context(), input.CandidateID)
	if err != nil {
		admin.operationError(writer, err)
		return
	}
	for index := range result.Changes {
		if !result.Changes[index].NeedsRestart {
			continue
		}
		reloaded, reloadErr := admin.manager.RequestReloadIfRunning()
		if reloadErr != nil {
			admin.internalError(writer, reloadErr)
			return
		}
		if !reloaded {
			result.Changes[index].Message += "; it will take effect the next time the managed core starts"
		}
		break
	}
	apiWriteJSON(writer, http.StatusOK, result)
}
