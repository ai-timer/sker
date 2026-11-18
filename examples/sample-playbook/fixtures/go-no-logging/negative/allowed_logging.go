package sample

func logRequest(logger Logger, requestID string) {
	logger.Info("request completed", "request_id", requestID)
}

type Logger interface {
	Info(message string, args ...any)
}
