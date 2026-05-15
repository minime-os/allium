#include <ctype.h>
#include <errno.h>
#include <poll.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <strings.h>
#include <sys/socket.h>
#include <sys/stat.h>
#include <sys/types.h>
#include <sys/un.h>
#include <sys/wait.h>
#include <time.h>
#include <unistd.h>

#define DEFAULT_SOCKET_PATH "/tmp/remotectl.sock"
#define DEFAULT_UINPUT_PATH "/dev/uinput"
#define ALT_UINPUT_PATH "/dev/input/uinput"
#define MAX_LINE 512
#define MAX_SEQ_SCRIPT 4096
#define MAX_STDIO_ARGV 32
#define DEFAULT_ARTIFACT_DIR "/mnt/sdcard/system/logs/remotectl"
#define DEFAULT_VISUAL_HOLD_MS 250
#define MAX_VISUAL_HOLD_MS 10000

static int ensure_dir_recursive(const char *path) {
  char tmp[256];
  size_t len;
  char *p;

  if (!path || !*path) {
    errno = EINVAL;
    return -1;
  }
  len = strlen(path);
  if (len >= sizeof(tmp)) {
    errno = ENAMETOOLONG;
    return -1;
  }

  memcpy(tmp, path, len + 1u);
  for (p = tmp + 1; *p; p++) {
    if (*p != '/')
      continue;
    *p = '\0';
    if (mkdir(tmp, 0755) != 0 && errno != EEXIST)
      return -1;
    *p = '/';
  }
  if (mkdir(tmp, 0755) != 0 && errno != EEXIST)
    return -1;
  return 0;
}

static int build_default_artifact_path(char *buf, size_t buf_len, const char *prefix,
                                       const char *ext) {
  time_t now;
  struct tm tmv;
  int n;

  if (!buf || buf_len == 0 || !prefix || !*prefix || !ext || !*ext) {
    errno = EINVAL;
    return -1;
  }

  if (ensure_dir_recursive(DEFAULT_ARTIFACT_DIR) != 0)
    return -1;

  now = time(NULL);
  if (now == (time_t)-1)
    return -1;
  if (!localtime_r(&now, &tmv))
    return -1;

  n = snprintf(buf, buf_len, "%s/%s-%04d%02d%02d-%02d%02d%02d.%s",
               DEFAULT_ARTIFACT_DIR, prefix, tmv.tm_year + 1900, tmv.tm_mon + 1,
               tmv.tm_mday, tmv.tm_hour, tmv.tm_min, tmv.tm_sec, ext);
  if (n <= 0 || (size_t)n >= buf_len) {
    errno = ENAMETOOLONG;
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

static int wait_readable(int fd, int timeout_ms) {
  struct pollfd pfd;
  int rc;

  if (timeout_ms < 0)
    return 0;

  memset(&pfd, 0, sizeof(pfd));
  pfd.fd = fd;
  pfd.events = POLLIN;
  for (;;) {
    rc = poll(&pfd, 1, timeout_ms);
    if (rc < 0 && errno == EINTR)
      continue;
    return rc;
  }
}

static int read_line_timeout(int fd, char *buf, size_t buf_len, int timeout_ms) {
  size_t len = 0;
  int overflow = 0;

  if (buf_len == 0)
    return -1;

  for (;;) {
    char ch;
    ssize_t n;
    int prc = wait_readable(fd, timeout_ms);
    if (prc == 0) {
      errno = ETIMEDOUT;
      return -1;
    }
    if (prc < 0)
      return -1;

    n = read(fd, &ch, 1);
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

  if (overflow) {
    errno = EMSGSIZE;
    return -1;
  }

  buf[len] = '\0';
  return 1;
}

static int connect_socket(const char *path) {
  int fd;
  struct sockaddr_un addr;

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

  if (connect(fd, (struct sockaddr *)&addr, sizeof(addr)) != 0) {
    close(fd);
    return -1;
  }
  return fd;
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

static int send_command(int fd, const char *cmd, char *reply, size_t reply_len,
                        int timeout_ms) {
  if (write_all(fd, cmd, strlen(cmd)) != 0)
    return -1;
  if (write_all(fd, "\n", 1) != 0)
    return -1;
  return read_line_timeout(fd, reply, reply_len, timeout_ms);
}

static int path_exists(const char *path) {
  struct stat st;
  return (stat(path, &st) == 0);
}

static int check_uinput_available(const char **which_path) {
  if (path_exists(DEFAULT_UINPUT_PATH)) {
    if (which_path)
      *which_path = DEFAULT_UINPUT_PATH;
    return 1;
  }
  if (path_exists(ALT_UINPUT_PATH)) {
    if (which_path)
      *which_path = ALT_UINPUT_PATH;
    return 1;
  }
  if (which_path)
    *which_path = NULL;
  return 0;
}

static int run_selftest(const char *socket_path, int timeout_ms) {
  const char *uinput_path = NULL;
  int ok = 1;
  int fd;
  char reply[MAX_LINE];

  if (check_uinput_available(&uinput_path)) {
    printf("OK uinput-path %s\n", uinput_path);
  } else {
    printf("ERR uinput-path missing (%s or %s)\n", DEFAULT_UINPUT_PATH,
           ALT_UINPUT_PATH);
    ok = 0;
  }

  if (path_exists(socket_path)) {
    printf("OK socket-path %s\n", socket_path);
  } else {
    printf("ERR socket-path missing %s\n", socket_path);
    ok = 0;
  }

  fd = connect_socket(socket_path);
  if (fd < 0) {
    perror("connect");
    return 1;
  }
  printf("OK socket-connect %s\n", socket_path);

  if (send_command(fd, "ping", reply, sizeof(reply), timeout_ms) <= 0) {
    perror("ping");
    close(fd);
    return 1;
  }
  printf("%s\n", reply);
  if (strncmp(reply, "OK", 2) != 0)
    ok = 0;

  if (send_command(fd, "status", reply, sizeof(reply), timeout_ms) <= 0) {
    perror("status");
    close(fd);
    return 1;
  }
  printf("%s\n", reply);
  if (strncmp(reply, "OK", 2) != 0)
    ok = 0;

  close(fd);
  return ok ? 0 : 1;
}

static int run_child_argv(char *const child_argv[]) {
  pid_t pid;
  int status;

  pid = fork();
  if (pid < 0) {
    perror("fork");
    return 1;
  }
  if (pid == 0) {
    execvp(child_argv[0], child_argv);
    perror(child_argv[0]);
    _exit(127);
  }

  for (;;) {
    if (waitpid(pid, &status, 0) < 0) {
      if (errno == EINTR)
        continue;
      perror("waitpid");
      return 1;
    }
    break;
  }

  if (WIFEXITED(status))
    return WEXITSTATUS(status);
  if (WIFSIGNALED(status))
    return 128 + WTERMSIG(status);
  return 1;
}

static int run_screenshot_exec(int argc, char **argv, int argi) {
  int i;
  int child_argc;
  char **child_argv;
  child_argc = (argc - argi) + 2;
  child_argv = (char **)calloc((size_t)child_argc + 1u, sizeof(char *));
  if (!child_argv) {
    perror("calloc");
    return 1;
  }

  child_argv[0] = (char *)"remote-screenrecctl";
  child_argv[1] = (char *)"screenshot";
  for (i = argi; i < argc; i++)
    child_argv[(i - argi) + 2] = argv[i];
  child_argv[child_argc] = NULL;
  execvp(child_argv[0], child_argv);
  perror("execvp remote-screenrecctl");
  free(child_argv);
  return 1;
}

static int split_simple_words(char *s, char **argv, int max_argv) {
  int argc = 0;
  while (*s) {
    while (*s && isspace((unsigned char)*s))
      s++;
    if (!*s)
      break;
    if (argc >= max_argv)
      return -1;
    argv[argc++] = s;
    while (*s && !isspace((unsigned char)*s))
      s++;
    if (*s) {
      *s = '\0';
      s++;
    }
  }
  return argc;
}

static int run_screenshot_stdio_line(char *cmd_line) {
  char *args[MAX_STDIO_ARGV + 2];
  char *child_argv[MAX_STDIO_ARGV + 4];
  int argc;
  int i;

  argc = split_simple_words(cmd_line, args, MAX_STDIO_ARGV + 1);
  if (argc < 0) {
    fprintf(stderr, "screenshot: too many args\n");
    return 2;
  }
  child_argv[0] = (char *)"remote-screenrecctl";
  child_argv[1] = (char *)"screenshot";
  for (i = 1; i < argc; i++)
    child_argv[i + 1] = args[i];
  child_argv[argc + 1] = NULL;

  return run_child_argv(child_argv);
}

static int run_screenrecctl_exec(int argc, char **argv, int argi, const char *subcmd) {
  int i;
  int child_argc = (argc - argi) + 2;
  int append_auto_path = 0;
  char auto_path[256];
  char **child_argv;

  if (!subcmd || !*subcmd) {
    errno = EINVAL;
    return 1;
  }

  if ((strcmp(subcmd, "start") == 0 || strcmp(subcmd, "capture") == 0) && argi >= argc) {
    if (build_default_artifact_path(auto_path, sizeof(auto_path), "screencast", "mp4") != 0) {
      perror("default screencast path");
      return 1;
    }
    append_auto_path = 1;
  }

  child_argv = (char **)calloc((size_t)child_argc + (size_t)append_auto_path + 1u,
                               sizeof(char *));
  if (!child_argv) {
    perror("calloc");
    return 1;
  }

  child_argv[0] = (char *)"remote-screenrecctl";
  child_argv[1] = (char *)subcmd;
  for (i = argi; i < argc; i++)
    child_argv[(i - argi) + 2] = argv[i];
  if (append_auto_path)
    child_argv[child_argc] = auto_path;
  child_argv[child_argc + append_auto_path] = NULL;

  if (append_auto_path)
    fprintf(stderr, "screencast -> %s\n", auto_path);

  execvp(child_argv[0], child_argv);
  perror("execvp remote-screenrecctl");
  free(child_argv);
  return 1;
}

static int run_screenrec_stdio_line(char *cmd_line) {
  char *args[MAX_STDIO_ARGV + 4];
  char *child_argv[MAX_STDIO_ARGV + 5];
  char auto_path[256];
  const char *subcmd = NULL;
  int argc;
  int i;
  int child_argc = 0;
  int append_auto_path = 0;

  argc = split_simple_words(cmd_line, args, MAX_STDIO_ARGV + 3);
  if (argc < 0) {
    fprintf(stderr, "capture: too many args\n");
    return 2;
  }
  if (argc < 1) {
    fprintf(stderr, "capture: missing command\n");
    return 2;
  }

  if (strcmp(args[0], "capture") == 0) {
    subcmd = "capture";
  } else if (strcmp(args[0], "capture-start") == 0) {
    subcmd = "start";
  } else if (strcmp(args[0], "capture-stop") == 0) {
    subcmd = "stop";
  } else if (strcmp(args[0], "capture-status") == 0) {
    subcmd = "status";
  } else if (strcmp(args[0], "capture-wait") == 0) {
    subcmd = "wait";
  } else if (strcmp(args[0], "capture-probe") == 0) {
    subcmd = "probe";
  } else {
    fprintf(stderr, "capture: unknown local command\n");
    return 2;
  }

  if ((strcmp(subcmd, "start") == 0 || strcmp(subcmd, "capture") == 0) && argc == 1) {
    if (build_default_artifact_path(auto_path, sizeof(auto_path), "screencast", "mp4") != 0) {
      perror("default screencast path");
      return 1;
    }
    append_auto_path = 1;
    fprintf(stderr, "screencast -> %s\n", auto_path);
  }

  child_argv[child_argc++] = (char *)"remote-screenrecctl";
  child_argv[child_argc++] = (char *)subcmd;
  for (i = 1; i < argc; i++)
    child_argv[child_argc++] = args[i];
  if (append_auto_path)
    child_argv[child_argc++] = auto_path;
  child_argv[child_argc] = NULL;

  return run_child_argv(child_argv);
}

static int parse_ms_value(const char *s, int min_ms, int max_ms, int *out_ms) {
  char *end = NULL;
  long v;

  if (!s || !*s || !out_ms)
    return -1;
  errno = 0;
  v = strtol(s, &end, 10);
  if (errno || end == s || *end != '\0' || v < min_ms || v > max_ms)
    return -1;
  *out_ms = (int)v;
  return 0;
}

static void sleep_ms(int ms) {
  struct timespec req;
  struct timespec rem;

  if (ms <= 0)
    return;
  req.tv_sec = ms / 1000;
  req.tv_nsec = (long)(ms % 1000) * 1000000L;
  while (nanosleep(&req, &rem) != 0) {
    if (errno != EINTR)
      break;
    req = rem;
  }
}

static int run_screenshot_child_with_optional_path(const char *out_path) {
  char *child_argv[4];
  int argc = 0;

  child_argv[argc++] = (char *)"remote-screenrecctl";
  child_argv[argc++] = (char *)"screenshot";
  if (out_path && *out_path)
    child_argv[argc++] = (char *)out_path;
  child_argv[argc] = NULL;
  return run_child_argv(child_argv);
}

static int run_visual_hold_sequence(const char *socket_path, int timeout_ms,
                                    const char *buttons, int hold_ms,
                                    const char *screenshot_path) {
  int fd = -1;
  int rc = 0;
  int down_ok = 0;
  char reply[MAX_LINE];
  char cmd[MAX_LINE];

  if (!buttons || !*buttons) {
    fprintf(stderr, "visual: missing button/combo\n");
    return 2;
  }
  if (hold_ms < 1 || hold_ms > MAX_VISUAL_HOLD_MS) {
    fprintf(stderr, "visual: invalid hold ms (1-%d)\n", MAX_VISUAL_HOLD_MS);
    return 2;
  }

  fd = connect_socket(socket_path);
  if (fd < 0) {
    perror("connect");
    return 1;
  }

  if (snprintf(cmd, sizeof(cmd), "down %s", buttons) >= (int)sizeof(cmd)) {
    fprintf(stderr, "visual: combo too long\n");
    close(fd);
    return 2;
  }
  if (send_command(fd, cmd, reply, sizeof(reply), timeout_ms) <= 0) {
    perror("send/read down");
    close(fd);
    return 1;
  }
  puts(reply);
  if (strncmp(reply, "OK", 2) != 0) {
    close(fd);
    return 1;
  }
  down_ok = 1;

  sleep_ms(hold_ms);

  if (run_screenshot_child_with_optional_path(screenshot_path) != 0)
    rc = 1;

  if (snprintf(cmd, sizeof(cmd), "up %s", buttons) >= (int)sizeof(cmd)) {
    fprintf(stderr, "visual: combo too long\n");
    rc = 1;
  } else if (down_ok) {
    if (send_command(fd, cmd, reply, sizeof(reply), timeout_ms) <= 0) {
      perror("send/read up");
      rc = 1;
    } else {
      puts(reply);
      if (strncmp(reply, "OK", 2) != 0)
        rc = 1;
    }
  }

  if (send_command(fd, "status", reply, sizeof(reply), timeout_ms) > 0) {
    puts(reply);
    if (strncmp(reply, "OK", 2) != 0)
      rc = 1;
  } else {
    perror("send/read status");
    rc = 1;
  }

  close(fd);
  return rc;
}

static int run_visual_exec(const char *socket_path, int timeout_ms, int argc, char **argv,
                           int argi) {
  const char *buttons;
  const char *shot_path = NULL;
  int hold_ms = DEFAULT_VISUAL_HOLD_MS;

  if (argi >= argc) {
    fprintf(stderr,
            "tap-visual/combo-visual usage: <BUTTON|COMBO> [hold_ms] [screenshot-path]\n");
    return 2;
  }

  buttons = argv[argi++];
  if (argi < argc) {
    if (parse_ms_value(argv[argi], 1, MAX_VISUAL_HOLD_MS, &hold_ms) == 0) {
      argi++;
    }
  }
  if (argi < argc) {
    shot_path = argv[argi++];
  }
  if (argi < argc) {
    fprintf(stderr,
            "tap-visual/combo-visual usage: <BUTTON|COMBO> [hold_ms] [screenshot-path]\n");
    return 2;
  }

  return run_visual_hold_sequence(socket_path, timeout_ms, buttons, hold_ms, shot_path);
}

static int run_visual_stdio_line(const char *socket_path, int timeout_ms, char *cmd_line) {
  char *args[MAX_STDIO_ARGV + 4];
  int argc;
  int hold_ms = DEFAULT_VISUAL_HOLD_MS;
  const char *buttons;
  const char *shot_path = NULL;
  int argi = 1;

  argc = split_simple_words(cmd_line, args, MAX_STDIO_ARGV + 3);
  if (argc < 0) {
    fprintf(stderr, "tap-visual: too many args\n");
    return 2;
  }
  if (argc < 2) {
    fprintf(stderr,
            "tap-visual/combo-visual usage: <BUTTON|COMBO> [hold_ms] [screenshot-path]\n");
    return 2;
  }

  buttons = args[argi++];
  if (argi < argc) {
    if (parse_ms_value(args[argi], 1, MAX_VISUAL_HOLD_MS, &hold_ms) == 0)
      argi++;
  }
  if (argi < argc)
    shot_path = args[argi++];
  if (argi < argc) {
    fprintf(stderr,
            "tap-visual/combo-visual usage: <BUTTON|COMBO> [hold_ms] [screenshot-path]\n");
    return 2;
  }

  return run_visual_hold_sequence(socket_path, timeout_ms, buttons, hold_ms, shot_path);
}

static int join_args(char *dst, size_t dst_len, int argc, char **argv, int argi) {
  size_t len = 0;
  int i;

  if (dst_len == 0)
    return -1;
  dst[0] = '\0';

  for (i = argi; i < argc; i++) {
    size_t part_len = strlen(argv[i]);
    if (len + part_len + (len ? 1u : 0u) + 1u > dst_len)
      return -1;
    if (len) {
      dst[len++] = ' ';
      dst[len] = '\0';
    }
    memcpy(dst + len, argv[i], part_len + 1u);
    len += part_len;
  }
  return 0;
}

static int run_seq(const char *socket_path, int timeout_ms, int argc, char **argv,
                   int argi) {
  char script[MAX_SEQ_SCRIPT];
  char reply[MAX_LINE];
  int fd;
  int rc = 0;
  char *p;

  if (argi >= argc) {
    fprintf(stderr, "seq requires a script string (use semicolons or newlines)\n");
    return 2;
  }
  if (join_args(script, sizeof(script), argc, argv, argi) != 0) {
    fprintf(stderr, "seq script too long\n");
    return 2;
  }

  fd = connect_socket(socket_path);
  if (fd < 0) {
    perror("connect");
    return 1;
  }

  p = script;
  while (*p) {
    char *start = p;
    char saved;
    char *cmd;

    while (*p && *p != ';' && *p != '\n')
      p++;
    saved = *p;
    *p = '\0';

    cmd = trim_spaces(start);
    if (*cmd) {
      if (send_command(fd, cmd, reply, sizeof(reply), timeout_ms) <= 0) {
        perror("send/read");
        rc = 1;
        break;
      }
      puts(reply);
      if (strncmp(reply, "ERR", 3) == 0) {
        rc = 1;
        break;
      }
    }

    if (saved == '\0')
      break;
    p++;
  }

  close(fd);
  return rc;
}

static void usage(const char *prog) {
  fprintf(stderr,
          "Usage: %s [--socket PATH] [--timeout-ms N] --stdio\n"
          "       %s [--socket PATH] [--timeout-ms N] selftest\n"
          "       %s [--socket PATH] [--timeout-ms N] seq \"tap DOWN; tap A\"\n"
          "       %s [--socket PATH] [--timeout-ms N] tap-visual <BTN|COMBO> [ms] [shot.png]\n"
          "       %s [--socket PATH] [--timeout-ms N] combo-visual <COMBO> [ms] [shot.png]\n"
          "       %s screenshot [remote-screenrecctl screenshot args...] (default path on SD if omitted)\n"
          "       %s capture [remote-screenrecctl args...] (default path on SD if omitted)\n"
          "       %s capture-start [args...] [out.mp4]\n"
          "       %s capture-stop|capture-status|capture-wait|capture-probe\n"
          "       %s [--socket PATH] [--timeout-ms N] <command> [args...]\n",
          prog, prog, prog, prog, prog, prog, prog, prog, prog, prog);
}

int main(int argc, char **argv) {
  const char *socket_path = DEFAULT_SOCKET_PATH;
  int timeout_ms = 5000;
  int stdio_mode = 0;
  int argi = 1;
  int fd;
  int exit_code = 0;

  while (argi < argc) {
    if (strcmp(argv[argi], "--socket") == 0) {
      if (argi + 1 >= argc) {
        usage(argv[0]);
        return 2;
      }
      socket_path = argv[++argi];
      argi++;
      continue;
    }
    if (strcmp(argv[argi], "--timeout-ms") == 0) {
      char *end = NULL;
      long v;
      if (argi + 1 >= argc) {
        usage(argv[0]);
        return 2;
      }
      errno = 0;
      v = strtol(argv[++argi], &end, 10);
      if (errno || end == argv[argi] || *end != '\0' || v < 0 || v > 600000) {
        fprintf(stderr, "invalid timeout\n");
        return 2;
      }
      timeout_ms = (int)v;
      argi++;
      continue;
    }
    if (strcmp(argv[argi], "--stdio") == 0) {
      stdio_mode = 1;
      argi++;
      continue;
    }
    if (strcmp(argv[argi], "-h") == 0 || strcmp(argv[argi], "--help") == 0) {
      usage(argv[0]);
      return 0;
    }
    break;
  }

  if (!stdio_mode && argi >= argc) {
    usage(argv[0]);
    return 2;
  }
  if (stdio_mode && argi < argc) {
    usage(argv[0]);
    return 2;
  }

  if (!stdio_mode && strcmp(argv[argi], "selftest") == 0)
    return run_selftest(socket_path, timeout_ms);

  if (!stdio_mode && strcmp(argv[argi], "screenshot") == 0)
    return run_screenshot_exec(argc, argv, argi + 1);

  if (!stdio_mode && strcmp(argv[argi], "capture") == 0)
    return run_screenrecctl_exec(argc, argv, argi + 1, "capture");

  if (!stdio_mode && strcmp(argv[argi], "capture-start") == 0)
    return run_screenrecctl_exec(argc, argv, argi + 1, "start");

  if (!stdio_mode && strcmp(argv[argi], "capture-stop") == 0)
    return run_screenrecctl_exec(argc, argv, argi + 1, "stop");

  if (!stdio_mode && strcmp(argv[argi], "capture-status") == 0)
    return run_screenrecctl_exec(argc, argv, argi + 1, "status");

  if (!stdio_mode && strcmp(argv[argi], "capture-wait") == 0)
    return run_screenrecctl_exec(argc, argv, argi + 1, "wait");

  if (!stdio_mode && strcmp(argv[argi], "capture-probe") == 0)
    return run_screenrecctl_exec(argc, argv, argi + 1, "probe");

  if (!stdio_mode && strcmp(argv[argi], "seq") == 0)
    return run_seq(socket_path, timeout_ms, argc, argv, argi + 1);

  if (!stdio_mode && strcmp(argv[argi], "tap-visual") == 0)
    return run_visual_exec(socket_path, timeout_ms, argc, argv, argi + 1);

  if (!stdio_mode && strcmp(argv[argi], "combo-visual") == 0)
    return run_visual_exec(socket_path, timeout_ms, argc, argv, argi + 1);

  fd = connect_socket(socket_path);
  if (fd < 0) {
    perror("connect");
    return 1;
  }

  if (stdio_mode) {
    char line[MAX_LINE];
    char reply[MAX_LINE];
    while (fgets(line, sizeof(line), stdin) != NULL) {
      char *cmd = trim_spaces(line);
      if (!*cmd)
        continue;
      if (strcasecmp(cmd, "quit") == 0 || strcasecmp(cmd, "exit") == 0)
        break;
      if (strncasecmp(cmd, "screenshot", 10) == 0 &&
          (cmd[10] == '\0' || isspace((unsigned char)cmd[10]))) {
        int sc_rc = run_screenshot_stdio_line(cmd);
        if (sc_rc == 0) {
          puts("OK screenshot");
        } else {
          printf("ERR screenshot-exit %d\n", sc_rc);
          exit_code = 1;
        }
        fflush(stdout);
        continue;
      }
      if ((strncasecmp(cmd, "capture", 7) == 0 &&
           (cmd[7] == '\0' || cmd[7] == '-' || isspace((unsigned char)cmd[7])))) {
        int cap_rc = run_screenrec_stdio_line(cmd);
        if (cap_rc == 0) {
          puts("OK capture");
        } else {
          printf("ERR capture-exit %d\n", cap_rc);
          exit_code = 1;
        }
        fflush(stdout);
        continue;
      }
      if (((strncasecmp(cmd, "tap-visual", 10) == 0 &&
            (cmd[10] == '\0' || isspace((unsigned char)cmd[10]))) ||
           (strncasecmp(cmd, "combo-visual", 12) == 0 &&
            (cmd[12] == '\0' || isspace((unsigned char)cmd[12]))))) {
        int vis_rc = run_visual_stdio_line(socket_path, timeout_ms, cmd);
        if (vis_rc == 0) {
          puts("OK visual");
        } else {
          printf("ERR visual-exit %d\n", vis_rc);
          exit_code = 1;
        }
        fflush(stdout);
        continue;
      }
      if (send_command(fd, cmd, reply, sizeof(reply), timeout_ms) <= 0) {
        perror("send/read");
        exit_code = 1;
        break;
      }
      puts(reply);
      fflush(stdout);
      if (strncmp(reply, "ERR", 3) == 0)
        exit_code = 1;
    }
    close(fd);
    return exit_code;
  }

  {
    char cmd[MAX_LINE];
    char reply[MAX_LINE];

    if (join_args(cmd, sizeof(cmd), argc, argv, argi) != 0) {
      fprintf(stderr, "command too long\n");
      close(fd);
      return 2;
    }

    if (send_command(fd, cmd, reply, sizeof(reply), timeout_ms) <= 0) {
      perror("send/read");
      close(fd);
      return 1;
    }
    puts(reply);
    close(fd);
    return (strncmp(reply, "ERR", 3) == 0) ? 1 : 0;
  }
}
