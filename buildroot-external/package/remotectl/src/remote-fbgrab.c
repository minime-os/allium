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
#include <sys/types.h>
#include <unistd.h>

#define DEFAULT_FB "/dev/fb0"

static void usage(const char *prog) {
  fprintf(stderr,
          "Usage: %s [--fb PATH] [--xrgb8888] <out.ppm>\n"
          "Dumps fbdev contents to binary PPM (P6).\n",
          prog);
}

static int write_all(int fd, const void *buf, size_t len) {
  const unsigned char *p = (const unsigned char *)buf;
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

static uint8_t scale_component(uint32_t v, uint32_t length) {
  if (length == 0)
    return 0;
  if (length >= 8)
    return (uint8_t)(v & 0xffu);
  return (uint8_t)((v * 255u) / ((1u << length) - 1u));
}

int main(int argc, char **argv) {
  const char *fb_path = DEFAULT_FB;
  const char *out_path = NULL;
  int force_xrgb8888 = 0;
  int fb_fd = -1;
  int out_fd = -1;
  struct fb_fix_screeninfo fix;
  struct fb_var_screeninfo var;
  uint8_t *map = NULL;
  size_t map_len = 0;
  int argi;
  int y;

  for (argi = 1; argi < argc; argi++) {
    if (strcmp(argv[argi], "--fb") == 0) {
      if (argi + 1 >= argc) {
        usage(argv[0]);
        return 2;
      }
      fb_path = argv[++argi];
      continue;
    }
    if (strcmp(argv[argi], "--xrgb8888") == 0) {
      force_xrgb8888 = 1;
      continue;
    }
    if (strcmp(argv[argi], "-h") == 0 || strcmp(argv[argi], "--help") == 0) {
      usage(argv[0]);
      return 0;
    }
    if (out_path) {
      usage(argv[0]);
      return 2;
    }
    out_path = argv[argi];
  }

  if (!out_path) {
    usage(argv[0]);
    return 2;
  }

  fb_fd = open(fb_path, O_RDONLY | O_CLOEXEC);
  if (fb_fd < 0) {
    perror("open fb");
    return 1;
  }

  memset(&fix, 0, sizeof(fix));
  memset(&var, 0, sizeof(var));
  if (ioctl(fb_fd, FBIOGET_FSCREENINFO, &fix) != 0) {
    perror("FBIOGET_FSCREENINFO");
    close(fb_fd);
    return 1;
  }
  if (ioctl(fb_fd, FBIOGET_VSCREENINFO, &var) != 0) {
    perror("FBIOGET_VSCREENINFO");
    close(fb_fd);
    return 1;
  }

  if (var.xres == 0 || var.yres == 0 || fix.line_length == 0) {
    fprintf(stderr, "invalid fb geometry\n");
    close(fb_fd);
    return 1;
  }

  map_len = (size_t)fix.line_length * (size_t)var.yres_virtual;
  if (map_len == 0) {
    fprintf(stderr, "invalid fb map length\n");
    close(fb_fd);
    return 1;
  }

  map = (uint8_t *)mmap(NULL, map_len, PROT_READ, MAP_SHARED, fb_fd, 0);
  if (map == MAP_FAILED) {
    perror("mmap");
    close(fb_fd);
    return 1;
  }

  out_fd = open(out_path, O_WRONLY | O_CREAT | O_TRUNC | O_CLOEXEC, 0644);
  if (out_fd < 0) {
    perror("open output");
    munmap(map, map_len);
    close(fb_fd);
    return 1;
  }

  {
    char header[64];
    int n = snprintf(header, sizeof(header), "P6\n%u %u\n255\n", var.xres, var.yres);
    if (n <= 0 || (size_t)n >= sizeof(header) || write_all(out_fd, header, (size_t)n) != 0) {
      perror("write header");
      close(out_fd);
      munmap(map, map_len);
      close(fb_fd);
      return 1;
    }
  }

  for (y = 0; y < (int)var.yres; y++) {
    uint8_t *row = map + (size_t)(y + var.yoffset) * (size_t)fix.line_length +
                   (size_t)var.xoffset * ((size_t)var.bits_per_pixel / 8u);
    unsigned int x;
    for (x = 0; x < var.xres; x++) {
      uint8_t rgb[3];
      if ((force_xrgb8888 || var.bits_per_pixel == 32) && var.bits_per_pixel >= 32) {
        uint32_t px = ((uint32_t *)row)[x];
        uint32_t r = (px >> (force_xrgb8888 ? 16 : var.red.offset)) & ((1u << (force_xrgb8888 ? 8 : var.red.length)) - 1u);
        uint32_t g = (px >> (force_xrgb8888 ? 8 : var.green.offset)) & ((1u << (force_xrgb8888 ? 8 : var.green.length)) - 1u);
        uint32_t b = (px >> (force_xrgb8888 ? 0 : var.blue.offset)) & ((1u << (force_xrgb8888 ? 8 : var.blue.length)) - 1u);
        rgb[0] = scale_component(r, force_xrgb8888 ? 8 : var.red.length);
        rgb[1] = scale_component(g, force_xrgb8888 ? 8 : var.green.length);
        rgb[2] = scale_component(b, force_xrgb8888 ? 8 : var.blue.length);
      } else if (var.bits_per_pixel == 16) {
        uint16_t px = ((uint16_t *)row)[x];
        uint32_t r = (px >> var.red.offset) & ((1u << var.red.length) - 1u);
        uint32_t g = (px >> var.green.offset) & ((1u << var.green.length) - 1u);
        uint32_t b = (px >> var.blue.offset) & ((1u << var.blue.length) - 1u);
        rgb[0] = scale_component(r, var.red.length);
        rgb[1] = scale_component(g, var.green.length);
        rgb[2] = scale_component(b, var.blue.length);
      } else {
        fprintf(stderr, "unsupported bpp=%u\n", var.bits_per_pixel);
        close(out_fd);
        munmap(map, map_len);
        close(fb_fd);
        return 1;
      }
      if (write_all(out_fd, rgb, sizeof(rgb)) != 0) {
        perror("write pixels");
        close(out_fd);
        munmap(map, map_len);
        close(fb_fd);
        return 1;
      }
    }
  }

  close(out_fd);
  munmap(map, map_len);
  close(fb_fd);
  return 0;
}
