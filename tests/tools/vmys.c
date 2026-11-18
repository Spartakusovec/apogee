/* vmys X Y [klik] – přesune virtuální ukazatel na absolutní pozici a volitelně klikne levým. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <time.h>
#include <wayland-client.h>
#include "virtual-pointer.h"
static struct zwlr_virtual_pointer_manager_v1 *m; static struct wl_seat *seat;
static void g(void *d, struct wl_registry *r, uint32_t n, const char *i, uint32_t v) {
  if (!strcmp(i, zwlr_virtual_pointer_manager_v1_interface.name)) m = wl_registry_bind(r, n, &zwlr_virtual_pointer_manager_v1_interface, 1);
  if (!strcmp(i, wl_seat_interface.name)) seat = wl_registry_bind(r, n, &wl_seat_interface, 1);
}
static void gr(void *d, struct wl_registry *r, uint32_t n) {}
static const struct wl_registry_listener rl = { g, gr };
static uint32_t t(void) { struct timespec s; clock_gettime(CLOCK_MONOTONIC, &s); return s.tv_sec * 1000 + s.tv_nsec / 1000000; }
int main(int c, char **v) {
  struct wl_display *d = wl_display_connect(NULL); if (!d) return 1;
  struct wl_registry *r = wl_display_get_registry(d); wl_registry_add_listener(r, &rl, NULL); wl_display_roundtrip(d);
  if (!m) { fprintf(stderr, "no virtual pointer\n"); return 1; }
  struct zwlr_virtual_pointer_v1 *p = zwlr_virtual_pointer_manager_v1_create_virtual_pointer(m, seat);
  int x = atoi(v[1]), y = atoi(v[2]);
  for (int k = 0; k < 2; k++) { /* první událost nového zařízení se zahodí */
    zwlr_virtual_pointer_v1_motion_absolute(p, t(), x, y, 1920, 1080); zwlr_virtual_pointer_v1_frame(p); wl_display_roundtrip(d); usleep(50000); }
  if (c > 3) {
    zwlr_virtual_pointer_v1_button(p, t(), 0x110, 1); zwlr_virtual_pointer_v1_frame(p); wl_display_roundtrip(d); usleep(60000);
    zwlr_virtual_pointer_v1_button(p, t(), 0x110, 0); zwlr_virtual_pointer_v1_frame(p); wl_display_roundtrip(d); usleep(60000);
  }
  return 0;
}
