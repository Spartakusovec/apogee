/* vkeyboard TEXT [enter] – types lower-case letters and digits (us layout) and optionally Enter. */
#define _GNU_SOURCE
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <sys/mman.h>
#include <time.h>
#include <wayland-client.h>
#include <xkbcommon/xkbcommon.h>
#include "virtual-keyboard.h"
static struct zwp_virtual_keyboard_manager_v1 *m; static struct wl_seat *seat;
static void g(void *d, struct wl_registry *r, uint32_t n, const char *i, uint32_t v) {
  if (!strcmp(i, zwp_virtual_keyboard_manager_v1_interface.name)) m = wl_registry_bind(r, n, &zwp_virtual_keyboard_manager_v1_interface, 1);
  if (!strcmp(i, wl_seat_interface.name)) seat = wl_registry_bind(r, n, &wl_seat_interface, 1);
}
static void gr(void *d, struct wl_registry *r, uint32_t n) {}
static const struct wl_registry_listener rl = { g, gr };
static uint32_t t(void) { struct timespec s; clock_gettime(CLOCK_MONOTONIC, &s); return s.tv_sec * 1000 + s.tv_nsec / 1000000; }
static int kod(char c) { const char *a = "qwertyuiop", *b = "asdfghjkl", *z = "zxcvbnm", *n = "1234567890";
  for (int i = 0; a[i]; i++) if (a[i] == c) return 16 + i; for (int i = 0; b[i]; i++) if (b[i] == c) return 30 + i;
  for (int i = 0; z[i]; i++) if (z[i] == c) return 44 + i; for (int i = 0; n[i]; i++) if (n[i] == c) return 2 + i; return -1; }
int main(int c, char **v) {
  struct wl_display *d = wl_display_connect(NULL); if (!d) return 1;
  struct wl_registry *r = wl_display_get_registry(d); wl_registry_add_listener(r, &rl, NULL); wl_display_roundtrip(d);
  if (!m) return 1;
  struct zwp_virtual_keyboard_v1 *k = zwp_virtual_keyboard_manager_v1_create_virtual_keyboard(m, seat);
  struct xkb_context *x = xkb_context_new(0); struct xkb_rule_names nm = { .layout = "us" };
  struct xkb_keymap *km = xkb_keymap_new_from_names(x, &nm, 0); char *s = xkb_keymap_get_as_string(km, XKB_KEYMAP_FORMAT_TEXT_V1);
  size_t len = strlen(s) + 1; int fd = memfd_create("km", 0); ftruncate(fd, len); char *p = mmap(0, len, PROT_WRITE, MAP_SHARED, fd, 0); memcpy(p, s, len);
  zwp_virtual_keyboard_v1_keymap(k, WL_KEYBOARD_KEYMAP_FORMAT_XKB_V1, fd, len); wl_display_roundtrip(d); usleep(100000);
  zwp_virtual_keyboard_v1_key(k, t(), 42, 1); zwp_virtual_keyboard_v1_key(k, t(), 42, 0); wl_display_roundtrip(d); usleep(50000);
  for (char *q = v[1]; *q; q++) { int kc = kod(*q); if (kc < 0) continue;
    zwp_virtual_keyboard_v1_key(k, t(), kc, 1); wl_display_roundtrip(d); usleep(20000);
    zwp_virtual_keyboard_v1_key(k, t(), kc, 0); wl_display_roundtrip(d); usleep(20000); }
  if (c > 2) { zwp_virtual_keyboard_v1_key(k, t(), 28, 1); wl_display_roundtrip(d); usleep(20000); zwp_virtual_keyboard_v1_key(k, t(), 28, 0); wl_display_roundtrip(d); usleep(50000); }
  return 0;
}
