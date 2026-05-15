#include <ctype.h>
#include <errno.h>
#include <fcntl.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

#include <zlib.h>

#define PNG_SIG_LEN 8

static const unsigned char k_png_sig[PNG_SIG_LEN] = {0x89, 'P', 'N', 'G', '\r', '\n', 0x1a, '\n'};

static void usage(const char *prog) {
  fprintf(stderr, "Usage: %s <input.ppm> <output.png>\n", prog);
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

static void put_u32be(unsigned char out[4], uint32_t v) {
  out[0] = (unsigned char)((v >> 24) & 0xffu);
  out[1] = (unsigned char)((v >> 16) & 0xffu);
  out[2] = (unsigned char)((v >> 8) & 0xffu);
  out[3] = (unsigned char)(v & 0xffu);
}

static int write_chunk(int fd, const char type[4], const unsigned char *data, uint32_t len) {
  unsigned char len_be[4];
  unsigned char crc_be[4];
  uLong crc;

  put_u32be(len_be, len);
  if (write_all(fd, len_be, sizeof(len_be)) != 0)
    return -1;
  if (write_all(fd, type, 4) != 0)
    return -1;
  if (len > 0 && write_all(fd, data, len) != 0)
    return -1;

  crc = crc32(0L, Z_NULL, 0);
  crc = crc32(crc, (const Bytef *)type, 4);
  if (len > 0)
    crc = crc32(crc, (const Bytef *)data, len);
  put_u32be(crc_be, (uint32_t)crc);
  if (write_all(fd, crc_be, sizeof(crc_be)) != 0)
    return -1;
  return 0;
}

static int read_token(FILE *fp, char *buf, size_t buf_len) {
  int c;
  size_t n = 0;

  if (buf_len == 0)
    return -1;

  for (;;) {
    c = fgetc(fp);
    if (c == EOF)
      return 0;
    if (isspace(c))
      continue;
    if (c == '#') {
      while ((c = fgetc(fp)) != EOF && c != '\n') {
      }
      if (c == EOF)
        return 0;
      continue;
    }
    break;
  }

  do {
    if (n + 1 >= buf_len) {
      errno = EMSGSIZE;
      return -1;
    }
    buf[n++] = (char)c;
    c = fgetc(fp);
  } while (c != EOF && !isspace(c));

  buf[n] = '\0';
  return 1;
}

static int parse_u32(const char *s, uint32_t *out) {
  char *end = NULL;
  unsigned long v;
  errno = 0;
  v = strtoul(s, &end, 10);
  if (errno || !end || *end != '\0')
    return -1;
  if (v > 0xffffffffUL)
    return -1;
  *out = (uint32_t)v;
  return 0;
}

int main(int argc, char **argv) {
  const char *in_path;
  const char *out_path;
  FILE *fp = NULL;
  int out_fd = -1;
  char tok[64];
  uint32_t w, h, maxv;
  size_t pixels_len;
  size_t raw_len;
  unsigned char *pixels = NULL;
  unsigned char *raw = NULL;
  unsigned char *comp = NULL;
  uLongf comp_len;
  uint32_t y;
  unsigned char ihdr[13];
  int rc = 1;

  if (argc != 3) {
    usage(argv[0]);
    return 2;
  }
  in_path = argv[1];
  out_path = argv[2];

  fp = fopen(in_path, "rb");
  if (!fp) {
    perror("fopen input");
    goto done;
  }

  if (read_token(fp, tok, sizeof(tok)) <= 0 || strcmp(tok, "P6") != 0) {
    fprintf(stderr, "invalid PPM magic (expected P6)\n");
    goto done;
  }
  if (read_token(fp, tok, sizeof(tok)) <= 0 || parse_u32(tok, &w) != 0 || w == 0) {
    fprintf(stderr, "invalid width\n");
    goto done;
  }
  if (read_token(fp, tok, sizeof(tok)) <= 0 || parse_u32(tok, &h) != 0 || h == 0) {
    fprintf(stderr, "invalid height\n");
    goto done;
  }
  if (read_token(fp, tok, sizeof(tok)) <= 0 || parse_u32(tok, &maxv) != 0 || maxv != 255) {
    fprintf(stderr, "unsupported maxval (expected 255)\n");
    goto done;
  }

  if ((size_t)w > (SIZE_MAX / 3u) / (size_t)h) {
    fprintf(stderr, "image too large\n");
    goto done;
  }
  pixels_len = (size_t)w * (size_t)h * 3u;
  raw_len = ((size_t)w * 3u + 1u) * (size_t)h;

  pixels = (unsigned char *)malloc(pixels_len);
  raw = (unsigned char *)malloc(raw_len);
  if (!pixels || !raw) {
    perror("malloc");
    goto done;
  }

  if (fread(pixels, 1, pixels_len, fp) != pixels_len) {
    fprintf(stderr, "short read from PPM pixel data\n");
    goto done;
  }

  for (y = 0; y < h; y++) {
    size_t src_off = (size_t)y * (size_t)w * 3u;
    size_t dst_off = (size_t)y * ((size_t)w * 3u + 1u);
    raw[dst_off] = 0; /* filter type 0 */
    memcpy(raw + dst_off + 1u, pixels + src_off, (size_t)w * 3u);
  }

  comp_len = compressBound((uLong)raw_len);
  comp = (unsigned char *)malloc((size_t)comp_len);
  if (!comp) {
    perror("malloc");
    goto done;
  }
  if (compress2((Bytef *)comp, &comp_len, (const Bytef *)raw, (uLong)raw_len, Z_BEST_SPEED) != Z_OK) {
    fprintf(stderr, "compress2 failed\n");
    goto done;
  }
  if (comp_len > 0xffffffffUL) {
    fprintf(stderr, "compressed image too large\n");
    goto done;
  }

  out_fd = open(out_path, O_WRONLY | O_CREAT | O_TRUNC | O_CLOEXEC, 0644);
  if (out_fd < 0) {
    perror("open output");
    goto done;
  }

  if (write_all(out_fd, k_png_sig, PNG_SIG_LEN) != 0) {
    perror("write png sig");
    goto done;
  }

  put_u32be(ihdr + 0, w);
  put_u32be(ihdr + 4, h);
  ihdr[8] = 8; /* bit depth */
  ihdr[9] = 2; /* color type RGB */
  ihdr[10] = 0; /* compression */
  ihdr[11] = 0; /* filter */
  ihdr[12] = 0; /* interlace */

  if (write_chunk(out_fd, "IHDR", ihdr, sizeof(ihdr)) != 0) {
    perror("write IHDR");
    goto done;
  }
  if (write_chunk(out_fd, "IDAT", comp, (uint32_t)comp_len) != 0) {
    perror("write IDAT");
    goto done;
  }
  if (write_chunk(out_fd, "IEND", NULL, 0) != 0) {
    perror("write IEND");
    goto done;
  }

  rc = 0;

done:
  if (out_fd >= 0)
    close(out_fd);
  if (fp)
    fclose(fp);
  free(comp);
  free(raw);
  free(pixels);
  return rc;
}
