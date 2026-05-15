#include "minarch.h"

Game game;

///////////////////////////////////////
// based on picoarch/unzip.c

#define ZIP_HEADER_SIZE 30
#define ZIP_CHUNK_SIZE 65536
#define ZIP_LE_READ16(buf) \
	((uint16_t)(((uint8_t *)(buf))[1] << 8 | ((uint8_t *)(buf))[0]))
#define ZIP_LE_READ32(buf) \
	((uint32_t)(((uint8_t *)(buf))[3] << 24 | \
	((uint8_t *)(buf))[2] << 16 | ((uint8_t *)(buf))[1] << 8 | \
	((uint8_t *)(buf))[0]))
typedef int (*Zip_extract_t)(FILE *zip, FILE *dst, size_t size);

static int Zip_copy(FILE *zip, FILE *dst, size_t size)
{
	uint8_t buffer[ZIP_CHUNK_SIZE];

	while (size) {
		size_t sz = MIN(size, ZIP_CHUNK_SIZE);

		if (sz != fread(buffer, 1, sz, zip))
			return -1;
		if (sz != fwrite(buffer, 1, sz, dst))
			return -1;
		size -= sz;
	}
	return 0;
}

static int Zip_inflate(FILE *zip, FILE *dst, size_t size)
{
	z_stream stream = {0};
	size_t have = 0;
	uint8_t in[ZIP_CHUNK_SIZE];
	uint8_t out[ZIP_CHUNK_SIZE];
	int ret = -1;

	ret = inflateInit2(&stream, -MAX_WBITS);
	if (ret != Z_OK)
		return ret;

	do {
		size_t insize = MIN(size, ZIP_CHUNK_SIZE);

		stream.avail_in = fread(in, 1, insize, zip);
		if (ferror(zip)) {
			(void)inflateEnd(&stream);
			return Z_ERRNO;
		}
		if (!stream.avail_in)
			break;
		stream.next_in = in;

		do {
			stream.avail_out = ZIP_CHUNK_SIZE;
			stream.next_out = out;

			ret = inflate(&stream, Z_NO_FLUSH);
			switch (ret) {
			case Z_NEED_DICT:
				ret = Z_DATA_ERROR;
			case Z_DATA_ERROR:
			case Z_MEM_ERROR:
				(void)inflateEnd(&stream);
				return ret;
			}

			have = ZIP_CHUNK_SIZE - stream.avail_out;
			if (fwrite(out, 1, have, dst) != have || ferror(dst)) {
				(void)inflateEnd(&stream);
				return Z_ERRNO;
			}
		} while (stream.avail_out == 0);

		size -= insize;
	} while (size && ret != Z_STREAM_END);

	(void)inflateEnd(&stream);

	if (!size || ret == Z_STREAM_END)
		return Z_OK;
	return Z_DATA_ERROR;
}

///////////////////////////////////////

const char *Game_storageRoot(void)
{
	const char *sd2 = getenv("SDCARD2_PATH");

	if (sd2 && sd2[0] && prefixMatch((char *)sd2, game.path))
		return sd2;
	return SDCARD_PATH;
}

void Game_open(char *path)
{
	LOG_info("Game_open\n");
	memset(&game, 0, sizeof(game));

	strcpy(game.path, path);
	strcpy(game.name, strrchr(path, '/') + 1);

	if (suffixMatch(".zip", game.path)) {
		LOG_info("is zip file\n");
		int supports_zip = 0;
		int i = 0;
		char *ext;
		char exts[128];
		char *extensions[32];

		strcpy(exts, core.extensions);
		while ((ext = strtok(i ? NULL : exts, "|"))) {
			extensions[i++] = ext;
			if (!strcmp("zip", ext)) {
				supports_zip = 1;
				break;
			}
		}
		extensions[i] = NULL;

		if (!supports_zip) {
			FILE *zip = fopen(game.path, "r");
			if (zip == NULL) {
				LOG_error("Error opening archive: %s\n\t%s\n",
					game.path, strerror(errno));
				return;
			}

			uint8_t header[ZIP_HEADER_SIZE];
			uint32_t next = 0;
			uint16_t len = 0;
			char filename[MAX_PATH];
			uint32_t compressed_size = 0;
			char extension[8];

			while (1) {
				if (next)
					fseek(zip, next, SEEK_CUR);

				if (ZIP_HEADER_SIZE != fread(header, 1, ZIP_HEADER_SIZE, zip))
					break;
				if ((uint16_t)(header[6]) & 0x0008)
					break;

				len = ZIP_LE_READ16(&header[26]);
				if (len >= MAX_PATH)
					break;

				if (len != fread(filename, 1, len, zip))
					break;
				filename[len] = '\0';
				LOG_info("filename: %s\n", filename);

				compressed_size = ZIP_LE_READ32(&header[18]);

				fseek(zip, ZIP_LE_READ16(&header[28]), SEEK_CUR);
				next = compressed_size;

				int found = 0;
				for (i = 0; extensions[i]; i++) {
					sprintf(extension, ".%s", extensions[i]);
					if (!suffixMatch(extension, filename))
						continue;
					found = 1;
					break;
				}
				if (!found)
					continue;

				char tmp_template[MAX_PATH];

				strcpy(tmp_template, "/tmp/minarch-XXXXXX");
				char *tmp_dirname = mkdtemp(tmp_template);

				sprintf(game.tmp_path, "%s/%s", tmp_dirname,
					basename(filename));

				FILE *dst = fopen(game.tmp_path, "w");
				if (dst == NULL) {
					game.tmp_path[0] = '\0';
					LOG_error("Error extracting file: %s\n\t%s\n",
						filename, strerror(errno));
					return;
				}

				Zip_extract_t extract = NULL;
				switch (ZIP_LE_READ16(&header[8])) {
				case 0: extract = Zip_copy; break;
				case 8: extract = Zip_inflate; break;
				}

				if (!extract || extract(zip, dst, compressed_size)) {
					game.tmp_path[0] = '\0';
					LOG_error("Error extracting file: %s\n\t%s\n",
						filename, strerror(errno));
					fclose(dst);
					return;
				}

				fclose(dst);
				break;
			}

			fclose(zip);
		}
	}

	if (!core.need_fullpath) {
		path = game.tmp_path[0] == '\0' ? game.path : game.tmp_path;

		FILE *file = fopen(path, "r");
		if (file == NULL) {
			LOG_error("Error opening game: %s\n\t%s\n", path,
				strerror(errno));
			return;
		}

		fseek(file, 0, SEEK_END);
		game.size = ftell(file);

		rewind(file);
		game.data = malloc(game.size);
		if (game.data == NULL) {
			LOG_error("Couldn't allocate memory for file: %s\n", path);
			fclose(file);
			return;
		}

		fread(game.data, sizeof(uint8_t), game.size, file);
		fclose(file);
	}

	char *tmp;
	char m3u_path[256];
	char base_path[256];
	char dir_name[256];

	strcpy(m3u_path, game.path);
	tmp = strrchr(m3u_path, '/') + 1;
	tmp[0] = '\0';

	strcpy(base_path, m3u_path);

	tmp = strrchr(m3u_path, '/');
	tmp[0] = '\0';

	tmp = strrchr(m3u_path, '/');
	strcpy(dir_name, tmp);

	tmp = m3u_path + strlen(m3u_path);
	strcpy(tmp, dir_name);

	tmp = m3u_path + strlen(m3u_path);
	strcpy(tmp, ".m3u");

	if (exists(m3u_path)) {
		strcpy(game.m3u_path, m3u_path);
		strcpy(game.name, strrchr(m3u_path, '/') + 1);
	}

	game.is_open = 1;
}

void Game_close(void)
{
	if (game.data)
		free(game.data);
	if (game.tmp_path[0])
		remove(game.tmp_path);
	game.is_open = 0;
	VIB_setStrength(0);
}

void Game_changeDisc(char *path)
{
	if (exactMatch(game.path, path) || !exists(path))
		return;

	Game_close();
	Game_open(path);

	struct retro_game_info game_info = {};
	game_info.path = game.path;
	game_info.data = game.data;
	game_info.size = game.size;

	disk_control_ext.replace_image_index(0, &game_info);
	putFile(CHANGE_DISC_PATH, path);
	Rewind_onStateChange();
}
