#include <errno.h>
#include <fcntl.h>
#include <linux/fb.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
#include <sys/mman.h>
#include <sys/stat.h>
#include <unistd.h>

#define DEFAULT_FB "/dev/fb0"
#define DEFAULT_BRIGHTNESS 10
#define SETTINGS_VERSION 4

struct opts {
  const char *fb_path;
  const char *image_path;
  const char *brightness_state_path;
  int brightness_default;
  int unblank;
  int overlay_firstboot_expand;
  int overlay_sd2_format;
  int animate;
};

struct fb_ctx {
  int fd;
  uint8_t *map;
  size_t map_len;
  int stride;
  int xres;
  int yres;
  int bpp;
  int red_offset;
  int green_offset;
  int blue_offset;
};

static int read_int_file(const char *path, int *out)
{
  FILE *f;
  int v;
  if (!path || !out)
    return -1;
  f = fopen(path, "r");
  if (!f)
    return -1;
  if (fscanf(f, "%d", &v) != 1) {
    fclose(f);
    return -1;
  }
  fclose(f);
  *out = v;
  return 0;
}

static int write_int_file(const char *path, int v)
{
  FILE *f = fopen(path, "w");
  if (!f)
    return -1;
  if (fprintf(f, "%d\n", v) < 0) {
    fclose(f);
    return -1;
  }
  fclose(f);
  return 0;
}

static uint32_t pack_rgb(const struct fb_ctx *fb, uint8_t r, uint8_t g, uint8_t b)
{
  return ((uint32_t)r << fb->red_offset) |
         ((uint32_t)g << fb->green_offset) |
         ((uint32_t)b << fb->blue_offset);
}

static void put_px(const struct fb_ctx *fb, int x, int y, uint32_t px)
{
  uint32_t *row;
  if (!fb || !fb->map)
    return;
  if (x < 0 || y < 0 || x >= fb->xres || y >= fb->yres)
    return;
  row = (uint32_t *)(fb->map + (y * fb->stride));
  row[x] = px;
}

static void fill_rect(const struct fb_ctx *fb, int x, int y, int w, int h, uint32_t px)
{
  int ix, iy;
  for (iy = 0; iy < h; iy++) {
    for (ix = 0; ix < w; ix++) {
      put_px(fb, x + ix, y + iy, px);
    }
  }
}

static void fill_pill(const struct fb_ctx *fb, int x, int y, int w, int h, uint32_t px)
{
  int iy;
  int r;

  if (w <= 0 || h <= 0)
    return;

  r = h / 2;
  if (r <= 0 || w <= h) {
    fill_rect(fb, x, y, w, h, px);
    return;
  }

  for (iy = 0; iy < h; iy++) {
    int dy = iy - r;
    int ady = (dy < 0) ? -dy : dy;
    int inset = 0;
    while (inset < r) {
      int rx = r - inset;
      if ((rx * rx) + (ady * ady) <= (r * r))
        break;
      inset++;
    }
    fill_rect(fb, x + inset, y + iy, w - (inset * 2), 1, px);
  }
}

static int glyph_rows(char c, uint8_t rows[7])
{
  memset(rows, 0, 7);
  if (c >= 'a' && c <= 'z')
    c = (char)(c - 'a' + 'A');
  switch (c) {
  case ' ': return 1;
  case ':': rows[1]=0x04; rows[4]=0x04; return 1;
  case '.': rows[6]=0x04; return 1;
  case '-': rows[3]=0x1F; return 1;
  case '0': rows[0]=0x0E; rows[1]=0x11; rows[2]=0x13; rows[3]=0x15; rows[4]=0x19; rows[5]=0x11; rows[6]=0x0E; return 1;
  case '1': rows[0]=0x04; rows[1]=0x0C; rows[2]=0x04; rows[3]=0x04; rows[4]=0x04; rows[5]=0x04; rows[6]=0x0E; return 1;
  case '2': rows[0]=0x0E; rows[1]=0x11; rows[2]=0x01; rows[3]=0x02; rows[4]=0x04; rows[5]=0x08; rows[6]=0x1F; return 1;
  case '3': rows[0]=0x1E; rows[1]=0x01; rows[2]=0x01; rows[3]=0x0E; rows[4]=0x01; rows[5]=0x01; rows[6]=0x1E; return 1;
  case '4': rows[0]=0x02; rows[1]=0x06; rows[2]=0x0A; rows[3]=0x12; rows[4]=0x1F; rows[5]=0x02; rows[6]=0x02; return 1;
  case '5': rows[0]=0x1F; rows[1]=0x10; rows[2]=0x10; rows[3]=0x1E; rows[4]=0x01; rows[5]=0x01; rows[6]=0x1E; return 1;
  case '6': rows[0]=0x0E; rows[1]=0x10; rows[2]=0x10; rows[3]=0x1E; rows[4]=0x11; rows[5]=0x11; rows[6]=0x0E; return 1;
  case '7': rows[0]=0x1F; rows[1]=0x01; rows[2]=0x02; rows[3]=0x04; rows[4]=0x08; rows[5]=0x08; rows[6]=0x08; return 1;
  case '8': rows[0]=0x0E; rows[1]=0x11; rows[2]=0x11; rows[3]=0x0E; rows[4]=0x11; rows[5]=0x11; rows[6]=0x0E; return 1;
  case '9': rows[0]=0x0E; rows[1]=0x11; rows[2]=0x11; rows[3]=0x0F; rows[4]=0x01; rows[5]=0x01; rows[6]=0x0E; return 1;
  case 'A': rows[0]=0x0E; rows[1]=0x11; rows[2]=0x11; rows[3]=0x1F; rows[4]=0x11; rows[5]=0x11; rows[6]=0x11; return 1;
  case 'B': rows[0]=0x1E; rows[1]=0x11; rows[2]=0x11; rows[3]=0x1E; rows[4]=0x11; rows[5]=0x11; rows[6]=0x1E; return 1;
  case 'C': rows[0]=0x0E; rows[1]=0x11; rows[2]=0x10; rows[3]=0x10; rows[4]=0x10; rows[5]=0x11; rows[6]=0x0E; return 1;
  case 'D': rows[0]=0x1C; rows[1]=0x12; rows[2]=0x11; rows[3]=0x11; rows[4]=0x11; rows[5]=0x12; rows[6]=0x1C; return 1;
  case 'E': rows[0]=0x1F; rows[1]=0x10; rows[2]=0x10; rows[3]=0x1E; rows[4]=0x10; rows[5]=0x10; rows[6]=0x1F; return 1;
  case 'F': rows[0]=0x1F; rows[1]=0x10; rows[2]=0x10; rows[3]=0x1E; rows[4]=0x10; rows[5]=0x10; rows[6]=0x10; return 1;
  case 'G': rows[0]=0x0E; rows[1]=0x11; rows[2]=0x10; rows[3]=0x17; rows[4]=0x11; rows[5]=0x11; rows[6]=0x0E; return 1;
  case 'H': rows[0]=0x11; rows[1]=0x11; rows[2]=0x11; rows[3]=0x1F; rows[4]=0x11; rows[5]=0x11; rows[6]=0x11; return 1;
  case 'I': rows[0]=0x0E; rows[1]=0x04; rows[2]=0x04; rows[3]=0x04; rows[4]=0x04; rows[5]=0x04; rows[6]=0x0E; return 1;
  case 'L': rows[0]=0x10; rows[1]=0x10; rows[2]=0x10; rows[3]=0x10; rows[4]=0x10; rows[5]=0x10; rows[6]=0x1F; return 1;
  case 'M': rows[0]=0x11; rows[1]=0x1B; rows[2]=0x15; rows[3]=0x15; rows[4]=0x11; rows[5]=0x11; rows[6]=0x11; return 1;
  case 'N': rows[0]=0x11; rows[1]=0x19; rows[2]=0x15; rows[3]=0x13; rows[4]=0x11; rows[5]=0x11; rows[6]=0x11; return 1;
  case 'O': rows[0]=0x0E; rows[1]=0x11; rows[2]=0x11; rows[3]=0x11; rows[4]=0x11; rows[5]=0x11; rows[6]=0x0E; return 1;
  case 'P': rows[0]=0x1E; rows[1]=0x11; rows[2]=0x11; rows[3]=0x1E; rows[4]=0x10; rows[5]=0x10; rows[6]=0x10; return 1;
  case 'R': rows[0]=0x1E; rows[1]=0x11; rows[2]=0x11; rows[3]=0x1E; rows[4]=0x14; rows[5]=0x12; rows[6]=0x11; return 1;
  case 'S': rows[0]=0x0F; rows[1]=0x10; rows[2]=0x10; rows[3]=0x0E; rows[4]=0x01; rows[5]=0x01; rows[6]=0x1E; return 1;
  case 'T': rows[0]=0x1F; rows[1]=0x04; rows[2]=0x04; rows[3]=0x04; rows[4]=0x04; rows[5]=0x04; rows[6]=0x04; return 1;
  case 'U': rows[0]=0x11; rows[1]=0x11; rows[2]=0x11; rows[3]=0x11; rows[4]=0x11; rows[5]=0x11; rows[6]=0x0E; return 1;
  case 'W': rows[0]=0x11; rows[1]=0x11; rows[2]=0x11; rows[3]=0x15; rows[4]=0x15; rows[5]=0x15; rows[6]=0x0A; return 1;
  case 'X': rows[0]=0x11; rows[1]=0x11; rows[2]=0x0A; rows[3]=0x04; rows[4]=0x0A; rows[5]=0x11; rows[6]=0x11; return 1;
  case 'Y': rows[0]=0x11; rows[1]=0x11; rows[2]=0x0A; rows[3]=0x04; rows[4]=0x04; rows[5]=0x04; rows[6]=0x04; return 1;
  case 'Z': rows[0]=0x1F; rows[1]=0x01; rows[2]=0x02; rows[3]=0x04; rows[4]=0x08; rows[5]=0x10; rows[6]=0x1F; return 1;
  default: rows[0]=0x1F; rows[3]=0x1F; rows[6]=0x1F; return 1;
  }
}

static void draw_char_5x7(const struct fb_ctx *fb, int x, int y, int scale,
                          char c, uint32_t fg, uint32_t bg)
{
  uint8_t rows[7];
  int gx, gy, dx, dy;
  if (!glyph_rows(c, rows))
    return;
  for (gy = 0; gy < 7; gy++) {
    for (gx = 0; gx < 5; gx++) {
      uint32_t px = (rows[gy] & (1u << (4 - gx))) ? fg : bg;
      for (dy = 0; dy < scale; dy++) {
        for (dx = 0; dx < scale; dx++) {
          put_px(fb, x + gx * scale + dx, y + gy * scale + dy, px);
        }
      }
    }
  }
}

static void draw_text_5x7(const struct fb_ctx *fb, int x, int y, int scale,
                          const char *text, uint32_t fg, uint32_t bg)
{
  int cursor = x;
  size_t i;
  if (!text)
    return;
  for (i = 0; text[i]; i++) {
    draw_char_5x7(fb, cursor, y, scale, text[i], fg, bg);
    cursor += (5 * scale) + scale;
  }
}

static int text_width_5x7_px(const char *text, int scale)
{
  size_t len;
  if (!text || scale <= 0)
    return 0;
  len = strlen(text);
  if (len == 0)
    return 0;
  return (int)(len * (6 * scale) - scale);
}

static void draw_overlay_progress(const struct fb_ctx *fb,
                                  const char *message,
                                  int frame)
{
  char line[96];
  size_t line_len;
  int dots = frame % 4;
  int scale = fb->xres >= 640 ? 3 : 2;
  int pad_x;
  int pad_y;
  int margin;
  int text_w;
  int pill_w;
  int pill_h;
  int pill_x;
  int pill_y;
  uint32_t bg = pack_rgb(fb, 255, 255, 255);
  uint32_t fg = pack_rgb(fb, 0, 0, 0);
  int i;
  int start_dots;

  if (!message)
    message = "";

  snprintf(line, sizeof(line), "%s....", message);
  line[sizeof(line) - 1] = '\0';
  line_len = strlen(line);
  start_dots = (line_len >= 4) ? (int)line_len - 4 : 0;
  for (i = start_dots + dots; i < start_dots + 4 && i < (int)line_len; i++)
    line[i] = ' ';

  for (;;) {
    pad_x = 7 * scale;
    pad_y = 4 * scale;
    margin = 12 * scale;
    text_w = text_width_5x7_px(line, scale);
    pill_w = text_w + (pad_x * 2);
    if (scale <= 1 || pill_w <= (fb->xres - (margin * 2)))
      break;
    scale--;
  }

  pill_h = (7 * scale) + (pad_y * 2);
  pill_x = (fb->xres - pill_w) / 2;
  pill_y = fb->yres - pill_h - margin;
  if (pill_y < 0)
    pill_y = 0;

  fill_pill(fb, pill_x, pill_y, pill_w, pill_h, bg);
  draw_text_5x7(fb, pill_x + ((pill_w - text_w) / 2), pill_y + pad_y, scale, line, fg, bg);
}

static void draw_overlay_firstboot_expand(const struct fb_ctx *fb, int frame)
{
  draw_overlay_progress(fb, "EXPANDING STORAGE", frame);
}

static void draw_overlay_sd2_format(const struct fb_ctx *fb, int frame)
{
  draw_overlay_progress(fb, "FORMATTING SECOND SD CARD", frame);
}

static int open_fb(struct fb_ctx *fb, const char *path)
{
  struct fb_fix_screeninfo finfo;
  struct fb_var_screeninfo vinfo;

  memset(fb, 0, sizeof(*fb));
  fb->fd = -1;
  fb->fd = open(path ? path : DEFAULT_FB, O_RDWR | O_CLOEXEC);
  if (fb->fd < 0)
    return -1;
  if (ioctl(fb->fd, FBIOGET_FSCREENINFO, &finfo) != 0)
    return -1;
  if (ioctl(fb->fd, FBIOGET_VSCREENINFO, &vinfo) != 0)
    return -1;
  if ((int)vinfo.bits_per_pixel != 32)
    return -1;

  fb->map_len = finfo.smem_len;
  fb->map = mmap(NULL, fb->map_len, PROT_READ | PROT_WRITE, MAP_SHARED, fb->fd, 0);
  if (fb->map == MAP_FAILED) {
    fb->map = NULL;
    return -1;
  }
  fb->stride = finfo.line_length;
  fb->xres = (int)vinfo.xres;
  fb->yres = (int)vinfo.yres;
  fb->bpp = (int)vinfo.bits_per_pixel;
  fb->red_offset = (int)vinfo.red.offset;
  fb->green_offset = (int)vinfo.green.offset;
  fb->blue_offset = (int)vinfo.blue.offset;
  return 0;
}

static void close_fb(struct fb_ctx *fb)
{
  if (!fb)
    return;
  if (fb->map)
    munmap(fb->map, fb->map_len);
  if (fb->fd >= 0)
    close(fb->fd);
  memset(fb, 0, sizeof(*fb));
  fb->fd = -1;
}

static int copy_file_to_fb(struct fb_ctx *fb, const char *path)
{
  int fd;
  ssize_t n;
  size_t off = 0;
  uint8_t buf[8192];
  if (!fb || !fb->map || !path)
    return -1;
  fd = open(path, O_RDONLY | O_CLOEXEC);
  if (fd < 0)
    return -1;
  while ((n = read(fd, buf, sizeof(buf))) > 0) {
    size_t want = (size_t)n;
    if (off >= fb->map_len)
      break;
    if (want > (fb->map_len - off))
      want = fb->map_len - off;
    memcpy(fb->map + off, buf, want);
    off += want;
    if (want < (size_t)n)
      break;
  }
  close(fd);
  return 0;
}

static int parse_brightness_from_settings(const char *path, int *brightness_out)
{
  uint8_t buf[8];
  int fd;
  int version;
  int brightness;
  ssize_t n;

  if (!path || !brightness_out)
    return -1;
  fd = open(path, O_RDONLY | O_CLOEXEC);
  if (fd < 0)
    return -1;
  n = read(fd, buf, sizeof(buf));
  close(fd);
  if (n < (ssize_t)sizeof(buf))
    return -1;

  version = (int)(buf[0] | (buf[1] << 8) | (buf[2] << 16) | (buf[3] << 24));
  brightness = (int)(buf[4] | (buf[5] << 8) | (buf[6] << 16) | (buf[7] << 24));
  if (version != SETTINGS_VERSION)
    return -1;
  if (brightness < 0)
    brightness = 0;
  if (brightness > 10)
    brightness = 10;
  *brightness_out = brightness;
  return 0;
}

static void apply_brightness_from_settings(const char *settings_path, int default_brightness)
{
  int level = default_brightness;
  int max_raw = 255;
  int raw;

  if (level < 0)
    level = 0;
  if (level > 10)
    level = 10;
  (void)parse_brightness_from_settings(settings_path, &level);

  if (read_int_file("/sys/class/backlight/backlight/max_brightness", &max_raw) != 0)
    (void)read_int_file("/sys/class/backlight/axp20x-backlight/max_brightness", &max_raw);
  if (max_raw <= 0)
    max_raw = 255;

  raw = (level * max_raw) / 10;
  if (raw < 1 && level > 0)
    raw = 1;
  if (raw > max_raw)
    raw = max_raw;

  if (write_int_file("/sys/class/backlight/backlight/brightness", raw) != 0)
    (void)write_int_file("/sys/class/backlight/axp20x-backlight/brightness", raw);
}

static void usage(const char *argv0)
{
  fprintf(stderr,
          "Usage: %s [--fb /dev/fb0] [--image file.fb] [--unblank]\\n"
          "          [--brightness-state /mnt/sdcard/.../settings.state] [--brightness-default 0..10]\\n"
          "          [--overlay firstboot-expand|sd2-format] [--animate]\\n",
          argv0);
}

static int parse_args(int argc, char **argv, struct opts *o)
{
  int i;
  memset(o, 0, sizeof(*o));
  o->fb_path = DEFAULT_FB;
  o->brightness_default = DEFAULT_BRIGHTNESS;

  for (i = 1; i < argc; i++) {
    if (!strcmp(argv[i], "--fb") && i + 1 < argc) {
      o->fb_path = argv[++i];
    } else if (!strcmp(argv[i], "--image") && i + 1 < argc) {
      o->image_path = argv[++i];
    } else if (!strcmp(argv[i], "--brightness-state") && i + 1 < argc) {
      o->brightness_state_path = argv[++i];
    } else if (!strcmp(argv[i], "--brightness-default") && i + 1 < argc) {
      o->brightness_default = atoi(argv[++i]);
      if (o->brightness_default < 0)
        o->brightness_default = 0;
      if (o->brightness_default > 10)
        o->brightness_default = 10;
    } else if (!strcmp(argv[i], "--overlay") && i + 1 < argc) {
      const char *v = argv[++i];
      if (!strcmp(v, "firstboot-expand")) {
        o->overlay_firstboot_expand = 1;
      } else if (!strcmp(v, "sd2-format")) {
        o->overlay_sd2_format = 1;
      } else {
        return -1;
      }
    } else if (!strcmp(argv[i], "--animate")) {
      o->animate = 1;
    } else if (!strcmp(argv[i], "--unblank")) {
      o->unblank = 1;
    } else if (!strcmp(argv[i], "-h") || !strcmp(argv[i], "--help")) {
      return 1;
    } else {
      return -1;
    }
  }
  return 0;
}

int main(int argc, char **argv)
{
  struct opts o;
  struct fb_ctx fb;
  int parse_rc;

  parse_rc = parse_args(argc, argv, &o);
  if (parse_rc == 1) {
    usage(argv[0]);
    return 0;
  }
  if (parse_rc != 0) {
    usage(argv[0]);
    return 1;
  }

  if (o.brightness_state_path)
    apply_brightness_from_settings(o.brightness_state_path, o.brightness_default);

  if (open_fb(&fb, o.fb_path) != 0) {
    close_fb(&fb);
    return 0;
  }

  if (o.unblank)
    (void)write_int_file("/sys/class/graphics/fb0/blank", 0);

  if (o.animate && (o.overlay_sd2_format || o.overlay_firstboot_expand)) {
    int frame = 0;
    for (;;) {
      if (o.image_path)
        (void)copy_file_to_fb(&fb, o.image_path);
      if (o.overlay_firstboot_expand)
        draw_overlay_firstboot_expand(&fb, frame);
      if (o.overlay_sd2_format)
        draw_overlay_sd2_format(&fb, frame);
      frame++;
      usleep(150000);
    }
  } else {
    if (o.image_path)
      (void)copy_file_to_fb(&fb, o.image_path);
    if (o.overlay_firstboot_expand)
      draw_overlay_firstboot_expand(&fb, 0);
    if (o.overlay_sd2_format)
      draw_overlay_sd2_format(&fb, 0);
  }

  close_fb(&fb);
  return 0;
}
