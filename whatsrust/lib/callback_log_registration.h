#ifndef CALLBACK_LOG_REGISTRATION_H
#define CALLBACK_LOG_REGISTRATION_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

typedef const char* JID;

typedef struct {
	char* id;
	JID chat;
	JID sender;
	char* pushName;
	bool mentionsSelf;
	int64_t timestamp;
	bool isFromMe;
	char* quoteID;
	uint16_t readBy;
	bool isForwarded;
	uint32_t forwardingScore;
} MessageInfo;

typedef struct {
	MessageInfo info;
	uint8_t messageType;
	void* message;
	uint8_t* forwardSource;
	size_t forwardSourceLen;
} Message;

typedef struct {
	uint8_t kind;
	void* data;
} Event;

typedef void (*EventCallback)(const Event*, void*);
typedef struct {
	EventCallback callback;
	void* user_data;
} EventHandler;

typedef void (*MessageHandlerCallback)(const Message*, bool, void*);
typedef struct {
	MessageHandlerCallback callback;
	void* user_data;
} MessageHandler;

typedef void (*PresenceHandlerCallback)(JID, bool, int64_t, void*);
typedef struct {
	PresenceHandlerCallback callback;
	void* user_data;
} PresenceHandler;

typedef void (*LogHandlerCallback)(const char*, uint8_t, void*);
typedef struct {
	LogHandlerCallback callback;
	void* user_data;
} LogHandler;

static void callLogInfo(LogHandler hdl, const char* msg, uint8_t level) {
	hdl.callback(msg, level, hdl.user_data);
}

#endif
