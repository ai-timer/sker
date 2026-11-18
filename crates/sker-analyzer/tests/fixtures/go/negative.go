package sample

func logRequest(logger Logger) {
	logger.Info("request completed", "request_id", requestID)
}
