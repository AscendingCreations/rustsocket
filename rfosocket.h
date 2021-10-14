#pragma once
#include <stdint.h>

int init_socket(const char *ipaddress,
                    uint64_t client_max,
                    int (*recv)(uint64_t, const char*, uint64_t),
                    int (*accept)(uint64_t, const char*),
                    int (*disconnect)(uint64_t));

int poll_events(void);
int socket_close(uint64_t index);
int socket_send(uint64_t index, const char *data, uint64_t size);
int socket_set_interest(uint64_t index, bool read);