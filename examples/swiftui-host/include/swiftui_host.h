#include <stdint.h>

typedef struct SwiftUIHostListener {
    void *context;
    void (*cards_changed)(void *context, uint32_t count);
    void (*card_opened)(void *context, uint32_t number);
} SwiftUIHostListener;

void *swiftui_host_view(void);
void swiftui_host_listen(SwiftUIHostListener listener);
void swiftui_host_add_card(void);
void swiftui_host_remove_card(uint32_t number);
void swiftui_host_clear(void);
