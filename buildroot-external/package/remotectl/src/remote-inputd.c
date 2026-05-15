#include <ctype.h>
#include <errno.h>
#include <fcntl.h>
#include <signal.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <strings.h>
#include <sys/socket.h>
#include <sys/stat.h>
#include <sys/time.h>
#include <sys/types.h>
#include <sys/un.h>
#include <time.h>
#include <unistd.h>

#include <linux/input.h>
#include <linux/uinput.h>

#define SRT_DEFAULT_SOCKET_PATH "/tmp/remotectl.sock"
#define SRT_DEVICE_NAME "Microsoft Remote Test Pad"
#define SRT_MAX_LINE 256
#define SRT_MAX_TAP_MS 10000
#define SRT_MAX_WAIT_MS 60000
#define SRT_MAX_COMBO_BUTTONS 8

struct button_map {
  const char *name;
  int code;
  int pressed;
};

static struct button_map g_buttons[] = {
    {"UP", BTN_DPAD_UP, 0},      {"DOWN", BTN_DPAD_DOWN, 0},
    {"LEFT", BTN_DPAD_LEFT, 0},  {"RIGHT", BTN_DPAD_RIGHT, 0},
    {"A", BTN_B, 0},             {"B", BTN_A, 0},
    {"X", BTN_Y, 0},             {"Y", BTN_X, 0},
    {"START", BTN_START, 0},     {"SELECT", BTN_SELECT, 0},
    {"MENU", BTN_MODE, 0},       {"L1", BTN_TL, 0},
    {"R1", BTN_TR, 0},           {"L2", BTN_TL2, 0},
    {"R2", BTN_TR2, 0},          {"L3", BTN_THUMBL, 0},
    {"R3", BTN_THUMBR, 0},       {"PLUS", KEY_VOLUMEUP, 0},
    {"MINUS", KEY_VOLUMEDOWN, 0},
};

#define BUTTON_COUNT ((int)(sizeof(g_buttons) / sizeof(g_buttons[0])))

struct app_ctx {
  int uinput_fd;
  int listen_fd;
  char socket_path[sizeof(((struct sockaddr_un *)0)->sun_path)];
};

static int set_button_down(struct app_ctx *ctx, struct button_map *btn);
static int set_button_up(struct app_ctx *ctx, struct button_map *btn);

static volatile sig_atomic_t g_stop = 0;

static void on_signal(int sig) {
  (void)sig;
  g_stop = 1;
}

static int ms_sleep(int ms) {
  struct timespec req;
  struct timespec rem;

  if (ms < 0)
    return -1;

  req.tv_sec = ms / 1000;
  req.tv_nsec = (long)(ms % 1000) * 1000000L;
  while (nanosleep(&req, &rem) != 0) {
    if (errno == EINTR) {
      if (g_stop)
        return -1;
      req = rem;
      continue;
    }
    return -1;
  }
  return 0;
}

static int write_all(int fd, const void *buf, size_t len) {
  const char *p = (const char *)buf;
  while (len > 0) {
    ssize_t n = write(fd, p, len);
    if (n < 0) {
      if (errno == EINTR)
        continue;
      return -1;
    }
    p += (size_t)n;
    len -= (size_t)n;
  }
  return 0;
}

static int send_reply(int fd, const char *line) {
  if (write_all(fd, line, strlen(line)) != 0)
    return -1;
  if (write_all(fd, "\n", 1) != 0)
    return -1;
  return 0;
}

static char *trim_spaces(char *s) {
  char *end;

  while (*s && isspace((unsigned char)*s))
    s++;
  if (!*s)
    return s;

  end = s + strlen(s) - 1;
  while (end > s && isspace((unsigned char)*end))
    *end-- = '\0';
  return s;
}

static int parse_int_range(const char *s, int min_value, int max_value, int *out) {
  char *end;
  long v;

  if (!s || !*s)
    return -1;
  errno = 0;
  v = strtol(s, &end, 10);
  if (errno || end == s || *end != '\0')
    return -1;
  if (v < min_value || v > max_value)
    return -1;
  *out = (int)v;
  return 0;
}

static struct button_map *find_button(const char *name) {
  int i;
  for (i = 0; i < BUTTON_COUNT; i++) {
    if (strcasecmp(name, g_buttons[i].name) == 0)
      return &g_buttons[i];
  }
  return NULL;
}

static int parse_button_combo(char *spec, struct button_map **out_buttons, int *out_count) {
  char *save = NULL;
  char *tok;
  int count = 0;
  int i;

  if (!spec || !*spec || !out_buttons || !out_count)
    return -1;

  for (tok = strtok_r(spec, "+", &save); tok != NULL; tok = strtok_r(NULL, "+", &save)) {
    struct button_map *btn;
    tok = trim_spaces(tok);
    if (!*tok)
      return -1;
    btn = find_button(tok);
    if (!btn)
      return -2;
    for (i = 0; i < count; i++) {
      if (out_buttons[i] == btn)
        return -3;
    }
    if (count >= SRT_MAX_COMBO_BUTTONS)
      return -4;
    out_buttons[count++] = btn;
  }

  if (count <= 0)
    return -1;
  *out_count = count;
  return 0;
}

static int set_combo_down(struct app_ctx *ctx, struct button_map **buttons, int count,
                          int *pressed_count_out) {
  int i;
  int pressed_now = 0;
  int changed[SRT_MAX_COMBO_BUTTONS];

  if (pressed_count_out)
    *pressed_count_out = 0;

  for (i = 0; i < SRT_MAX_COMBO_BUTTONS; i++)
    changed[i] = 0;

  for (i = 0; i < count; i++) {
    changed[i] = buttons[i]->pressed ? 0 : 1;
    if (set_button_down(ctx, buttons[i]) != 0) {
      int j;
      for (j = i - 1; j >= 0; j--)
        if (changed[j])
          (void)set_button_up(ctx, buttons[j]);
      return -1;
    }
    if (changed[i])
      pressed_now++;
  }

  if (pressed_count_out)
    *pressed_count_out = pressed_now;
  return 0;
}

static int set_combo_up(struct app_ctx *ctx, struct button_map **buttons, int count) {
  int i;
  for (i = count - 1; i >= 0; i--) {
    if (set_button_up(ctx, buttons[i]) != 0)
      return -1;
  }
  return 0;
}

static int emit_input_event(int fd, uint16_t type, uint16_t code, int32_t value) {
  struct input_event ev;
  memset(&ev, 0, sizeof(ev));
  gettimeofday(&ev.time, NULL);
  ev.type = type;
  ev.code = code;
  ev.value = value;
  return write_all(fd, &ev, sizeof(ev));
}

static int emit_syn(int fd) { return emit_input_event(fd, EV_SYN, SYN_REPORT, 0); }

static int send_button_event(int fd, int code, int value) {
  if (emit_input_event(fd, EV_KEY, (uint16_t)code, value) != 0)
    return -1;
  if (emit_syn(fd) != 0)
    return -1;
  return 0;
}

static int set_button_down(struct app_ctx *ctx, struct button_map *btn) {
  if (btn->pressed)
    return 0;
  if (send_button_event(ctx->uinput_fd, btn->code, 1) != 0)
    return -1;
  btn->pressed = 1;
  return 0;
}

static int set_button_up(struct app_ctx *ctx, struct button_map *btn) {
  if (!btn->pressed)
    return 0;
  if (send_button_event(ctx->uinput_fd, btn->code, 0) != 0)
    return -1;
  btn->pressed = 0;
  return 0;
}

static int flush_buttons(struct app_ctx *ctx) {
  int i;
  int changed = 0;

  for (i = 0; i < BUTTON_COUNT; i++) {
    if (!g_buttons[i].pressed)
      continue;
    if (emit_input_event(ctx->uinput_fd, EV_KEY, (uint16_t)g_buttons[i].code, 0) != 0)
      return -1;
    g_buttons[i].pressed = 0;
    changed = 1;
  }

  if (changed && emit_syn(ctx->uinput_fd) != 0)
    return -1;
  return 0;
}

static int pressed_count(void) {
  int i;
  int count = 0;
  for (i = 0; i < BUTTON_COUNT; i++) {
    if (g_buttons[i].pressed)
      count++;
  }
  return count;
}

static int open_uinput_fd(void) {
  static const char *paths[] = {"/dev/uinput", "/dev/input/uinput", NULL};
  int i;
  for (i = 0; paths[i] != NULL; i++) {
    int fd = open(paths[i], O_WRONLY | O_NONBLOCK | O_CLOEXEC);
    if (fd >= 0)
      return fd;
  }
  return -1;
}

static int create_virtual_pad(int fd) {
  struct uinput_user_dev dev;
  int i;

  if (ioctl(fd, UI_SET_EVBIT, EV_KEY) < 0)
    return -1;
  if (ioctl(fd, UI_SET_EVBIT, EV_ABS) < 0)
    return -1;

  for (i = 0; i < BUTTON_COUNT; i++) {
    if (ioctl(fd, UI_SET_KEYBIT, g_buttons[i].code) < 0)
      return -1;
  }

  if (ioctl(fd, UI_SET_ABSBIT, ABS_X) < 0)
    return -1;
  if (ioctl(fd, UI_SET_ABSBIT, ABS_Y) < 0)
    return -1;
  if (ioctl(fd, UI_SET_ABSBIT, ABS_RX) < 0)
    return -1;
  if (ioctl(fd, UI_SET_ABSBIT, ABS_RY) < 0)
    return -1;

  memset(&dev, 0, sizeof(dev));
  snprintf(dev.name, sizeof(dev.name), "%s", SRT_DEVICE_NAME);
  dev.id.bustype = BUS_USB;
  dev.id.vendor = 0x045e; /* Microsoft */
  dev.id.product = 0x028e; /* Xbox 360 Controller */
  dev.id.version = 1;

  dev.absmin[ABS_X] = -32768;
  dev.absmax[ABS_X] = 32767;
  dev.absmin[ABS_Y] = -32768;
  dev.absmax[ABS_Y] = 32767;
  dev.absmin[ABS_RX] = -32768;
  dev.absmax[ABS_RX] = 32767;
  dev.absmin[ABS_RY] = -32768;
  dev.absmax[ABS_RY] = 32767;

  if (write_all(fd, &dev, sizeof(dev)) != 0)
    return -1;
  if (ioctl(fd, UI_DEV_CREATE) < 0)
    return -1;

  /* Give udev/kernel a moment to publish the event node before MinUI scans. */
  ms_sleep(100);

  if (emit_input_event(fd, EV_ABS, ABS_X, 0) != 0)
    return -1;
  if (emit_input_event(fd, EV_ABS, ABS_Y, 0) != 0)
    return -1;
  if (emit_input_event(fd, EV_ABS, ABS_RX, 0) != 0)
    return -1;
  if (emit_input_event(fd, EV_ABS, ABS_RY, 0) != 0)
    return -1;
  if (emit_syn(fd) != 0)
    return -1;

  return 0;
}

static int create_server_socket(const char *path) {
  int fd;
  struct sockaddr_un addr;

  if (!path || !*path) {
    errno = EINVAL;
    return -1;
  }
  if (strlen(path) >= sizeof(addr.sun_path)) {
    errno = ENAMETOOLONG;
    return -1;
  }

  fd = socket(AF_UNIX, SOCK_STREAM, 0);
  if (fd < 0)
    return -1;

  memset(&addr, 0, sizeof(addr));
  addr.sun_family = AF_UNIX;
  snprintf(addr.sun_path, sizeof(addr.sun_path), "%s", path);

  unlink(path);
  if (bind(fd, (struct sockaddr *)&addr, sizeof(addr)) < 0) {
    close(fd);
    return -1;
  }
  if (chmod(path, 0600) < 0) {
    close(fd);
    unlink(path);
    return -1;
  }
  if (listen(fd, 4) < 0) {
    close(fd);
    unlink(path);
    return -1;
  }

  return fd;
}

static int read_line(int fd, char *buf, size_t buf_len) {
  size_t len = 0;
  int overflow = 0;

  if (buf_len == 0)
    return -1;

  for (;;) {
    char ch;
    ssize_t n = read(fd, &ch, 1);
    if (n == 0) {
      if (len == 0 && !overflow)
        return 0;
      break;
    }
    if (n < 0) {
      if (errno == EINTR)
        continue;
      return -1;
    }

    if (ch == '\r')
      continue;
    if (ch == '\n')
      break;

    if (!overflow) {
      if (len + 1 < buf_len) {
        buf[len++] = ch;
      } else {
        overflow = 1;
      }
    }
  }

  if (overflow)
    return -2;

  buf[len] = '\0';
  return 1;
}

static int handle_command(struct app_ctx *ctx, int client_fd, char *line) {
  char *save = NULL;
  char *cmd;
  char *arg1;
  char *arg2;
  char reply[128];

  line = trim_spaces(line);
  if (!*line)
    return send_reply(client_fd, "OK");

  cmd = strtok_r(line, " \t", &save);
  if (!cmd)
    return send_reply(client_fd, "ERR parse");

  if (strcasecmp(cmd, "ping") == 0) {
    return send_reply(client_fd, "OK pong");
  }
  if (strcasecmp(cmd, "help") == 0) {
    return send_reply(client_fd,
                      "OK commands: ping status help down up tap hold sleep wait flush quit (buttons or combos like SELECT+A)");
  }
  if (strcasecmp(cmd, "status") == 0) {
    snprintf(reply, sizeof(reply), "OK running pressed=%d", pressed_count());
    return send_reply(client_fd, reply);
  }
  if (strcasecmp(cmd, "flush") == 0) {
    if (flush_buttons(ctx) != 0)
      return send_reply(client_fd, "ERR uinput-write");
    return send_reply(client_fd, "OK flushed");
  }
  if (strcasecmp(cmd, "quit") == 0) {
    if (send_reply(client_fd, "OK bye") != 0)
      return -1;
    return 1;
  }
  if (strcasecmp(cmd, "sleep") == 0 || strcasecmp(cmd, "wait") == 0) {
    int delay_ms;
    arg1 = strtok_r(NULL, " \t", &save);
    if (!arg1)
      return send_reply(client_fd, "ERR missing-ms");
    if (parse_int_range(arg1, 0, SRT_MAX_WAIT_MS, &delay_ms) != 0)
      return send_reply(client_fd, "ERR invalid-ms");
    if (ms_sleep(delay_ms) != 0)
      return send_reply(client_fd, "ERR sleep");
    return send_reply(client_fd, "OK");
  }

  if (strcasecmp(cmd, "down") == 0 || strcasecmp(cmd, "up") == 0 ||
      strcasecmp(cmd, "tap") == 0 || strcasecmp(cmd, "hold") == 0) {
    struct button_map *combo[SRT_MAX_COMBO_BUTTONS];
    int combo_count = 0;
    int tap_ms = 60;
    int is_hold = (strcasecmp(cmd, "hold") == 0);
    int i;
    int parse_rc;

    arg1 = strtok_r(NULL, " \t", &save);
    if (!arg1)
      return send_reply(client_fd, "ERR missing-button");

    parse_rc = parse_button_combo(arg1, combo, &combo_count);
    if (parse_rc == -2)
      return send_reply(client_fd, "ERR unknown-button");
    if (parse_rc == -3)
      return send_reply(client_fd, "ERR duplicate-button");
    if (parse_rc == -4)
      return send_reply(client_fd, "ERR combo-too-large");
    if (parse_rc != 0)
      return send_reply(client_fd, "ERR invalid-combo");

    if (strcasecmp(cmd, "down") == 0) {
      if (set_combo_down(ctx, combo, combo_count, NULL) != 0)
        return send_reply(client_fd, "ERR uinput-write");
      return send_reply(client_fd, "OK");
    }

    if (strcasecmp(cmd, "up") == 0) {
      if (set_combo_up(ctx, combo, combo_count) != 0)
        return send_reply(client_fd, "ERR uinput-write");
      return send_reply(client_fd, "OK");
    }

    arg2 = strtok_r(NULL, " \t", &save);
    if (is_hold && !arg2)
      return send_reply(client_fd, "ERR missing-ms");
    if (arg2 &&
        parse_int_range(arg2, 20, is_hold ? SRT_MAX_WAIT_MS : SRT_MAX_TAP_MS, &tap_ms) != 0)
      return send_reply(client_fd, "ERR invalid-ms");
    for (i = 0; i < combo_count; i++) {
      if (combo[i]->pressed)
        return send_reply(client_fd, "ERR button-already-down");
    }
    if (set_combo_down(ctx, combo, combo_count, NULL) != 0)
      return send_reply(client_fd, "ERR uinput-write");
    if (ms_sleep(tap_ms) != 0)
      return send_reply(client_fd, "ERR sleep");
    if (set_combo_up(ctx, combo, combo_count) != 0)
      return send_reply(client_fd, "ERR uinput-write");
    return send_reply(client_fd, "OK");
  }

  return send_reply(client_fd, "ERR unknown-command");
}

static void handle_client(struct app_ctx *ctx, int client_fd) {
  char line[SRT_MAX_LINE];

  for (;;) {
    int rc;
    rc = read_line(client_fd, line, sizeof(line));
    if (rc == 0)
      break;
    if (rc == -1)
      break;
    if (rc == -2) {
      if (send_reply(client_fd, "ERR line-too-long") != 0)
        break;
      continue;
    }

    rc = handle_command(ctx, client_fd, line);
    if (rc == 1)
      break;
    if (rc < 0)
      break;
    if (g_stop)
      break;
  }
}

static void release_and_destroy(struct app_ctx *ctx) {
  if (!ctx)
    return;
  if (ctx->uinput_fd >= 0) {
    flush_buttons(ctx);
    ioctl(ctx->uinput_fd, UI_DEV_DESTROY);
    close(ctx->uinput_fd);
    ctx->uinput_fd = -1;
  }
  if (ctx->listen_fd >= 0) {
    close(ctx->listen_fd);
    ctx->listen_fd = -1;
  }
  if (ctx->socket_path[0])
    unlink(ctx->socket_path);
}

static void usage(const char *prog) {
  fprintf(stderr, "Usage: %s [--socket PATH]\n", prog);
}

int main(int argc, char **argv) {
  struct app_ctx ctx;
  int i;

  memset(&ctx, 0, sizeof(ctx));
  ctx.uinput_fd = -1;
  ctx.listen_fd = -1;
  snprintf(ctx.socket_path, sizeof(ctx.socket_path), "%s", SRT_DEFAULT_SOCKET_PATH);

  for (i = 1; i < argc; i++) {
    if (strcmp(argv[i], "--socket") == 0) {
      if (i + 1 >= argc) {
        usage(argv[0]);
        return 2;
      }
      i++;
      if (strlen(argv[i]) >= sizeof(ctx.socket_path)) {
        fprintf(stderr, "socket path too long\n");
        return 2;
      }
      snprintf(ctx.socket_path, sizeof(ctx.socket_path), "%s", argv[i]);
      continue;
    }
    if (strcmp(argv[i], "-h") == 0 || strcmp(argv[i], "--help") == 0) {
      usage(argv[0]);
      return 0;
    }
    usage(argv[0]);
    return 2;
  }

  signal(SIGPIPE, SIG_IGN);
  signal(SIGINT, on_signal);
  signal(SIGTERM, on_signal);

  ctx.uinput_fd = open_uinput_fd();
  if (ctx.uinput_fd < 0) {
    perror("open /dev/uinput");
    return 1;
  }
  if (create_virtual_pad(ctx.uinput_fd) != 0) {
    perror("create virtual pad");
    release_and_destroy(&ctx);
    return 1;
  }

  ctx.listen_fd = create_server_socket(ctx.socket_path);
  if (ctx.listen_fd < 0) {
    perror("create server socket");
    release_and_destroy(&ctx);
    return 1;
  }

  fprintf(stderr, "remote-inputd: listening on %s\n", ctx.socket_path);

  while (!g_stop) {
    int cfd = accept(ctx.listen_fd, NULL, NULL);
    if (cfd < 0) {
      if (errno == EINTR)
        continue;
      perror("accept");
      break;
    }
    handle_client(&ctx, cfd);
    close(cfd);
  }

  release_and_destroy(&ctx);
  return 0;
}
