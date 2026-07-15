#ifndef RUST_DOTNET_ASYNC_CALLBACK_H
#define RUST_DOTNET_ASYNC_CALLBACK_H

#include <stdint.h>

typedef struct ac_registration ac_registration;
typedef int (*ac_callback)(void *context, int value);

int ac_register(
    ac_callback callback,
    void *context,
    int fail_first_unregister,
    ac_registration **out_registration
);
int ac_unregister(ac_registration *registration);
int ac_live_workers(void);
int ac_copy_utf16(const uint16_t *input, uint16_t **output);
void ac_free_utf16(uint16_t *value);

#endif
