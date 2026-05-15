#define _GNU_SOURCE
/*
 * MinUI local glue for packaged core metadata and BIOS/default-core lookup.
 * This is intentionally separate from the upstream import.
 */
#include <ctype.h>
#include <dirent.h>
#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>

#include "defines.h"
#include "core_registry.h"
#include "utils.h"

#define CORE_REGISTRY_SYSTEMS_CFG SYSTEM_PATH "/systems.cfg"
#define CORE_REGISTRY_CORES_DIR CORE_CONFIGS_PATH
#define CORE_REGISTRY_DEFAULT_CORES_CFG USERDATA_PATH "/emulation/default-cores.cfg"

///////////////////////////////////////
static void core_registry_trim(char* s) {
	char* start;
	char* end;
	if (!s) return;
	start = s;
	while (*start && isspace((unsigned char)*start)) start++;
	if (start != s) memmove(s, start, strlen(start) + 1);
	end = s + strlen(s);
	while (end > s && isspace((unsigned char)*(end - 1))) end--;
	*end = '\0';
}

static void core_registry_split_csv(char* value, char out[][32], int* count, int max_count) {
	char* token;
	if (!value || !count) return;
	*count = 0;
	token = strtok(value, ",");
	while (token && *count < max_count) {
		while (*token && isspace((unsigned char)*token)) token++;
		char* end = token + strlen(token);
		while (end > token && isspace((unsigned char)*(end - 1))) end--;
		*end = '\0';
		if (*token) {
			snprintf(out[*count], 32, "%s", token);
			(*count)++;
		}
		token = strtok(NULL, ",");
	}
}

static void core_registry_parse_bios_items(CoreRegistryCore* core, char* value, int group, int optional, const char* system_id) {
	char* token;
	if (!core || !value) return;
	token = strtok(value, ",");
	while (token && core->bios_rule_count < CORE_REGISTRY_MAX_BIOS_RULES) {
		char* sep;
		char* filename = token;
		char* md5 = NULL;
		while (*filename && isspace((unsigned char)*filename)) filename++;
		sep = strchr(filename, ':');
		if (sep) {
			*sep = '\0';
			md5 = sep + 1;
		}
		core_registry_trim(filename);
		if (md5) core_registry_trim(md5);
		if (*filename) {
			CoreRegistryBiosRule* rule = &core->bios_rules[core->bios_rule_count++];
			snprintf(rule->filename, sizeof(rule->filename), "%s", filename);
			rule->md5[0] = '\0';
			if (md5 && strlen(md5) == 32) {
				for (int i=0; i<32; i++) {
					rule->md5[i] = (char)tolower((unsigned char)md5[i]);
				}
				rule->md5[32] = '\0';
			}
			rule->system_id[0] = '\0';
			if (system_id && system_id[0]) {
				snprintf(rule->system_id, sizeof(rule->system_id), "%s", system_id);
			}
			rule->group = group;
			rule->optional = optional ? 1 : 0;
		}
		token = strtok(NULL, ",");
	}
}

static int core_registry_key_with_scope(const char* key, const char* base, char system_id[32]) {
	size_t base_len;
	if (!key || !base || !system_id) return 0;
	system_id[0] = '\0';
	if (exactMatch((char*)key, (char*)base)) return 1;
	base_len = strlen(base);
	if (strncmp(key, base, base_len) != 0) return 0;
	if (key[base_len] != '.' || !key[base_len + 1]) return 0;
	snprintf(system_id, 32, "%s", key + base_len + 1);
	return 1;
}

///////////////////////////////////////
static int core_registry_parse_core_manifest(const char* path, CoreRegistryCore* core) {
	FILE* file;
	char line[512];
	int bios_any_group = 0;
	if (!path || !core) return -1;
	memset(core, 0, sizeof(*core));
	core->default_priority = 100;

	file = fopen(path, "r");
	if (!file) return -1;
	while (fgets(line, sizeof(line), file)) {
		char* key;
		char* value;
		normalizeNewline(line);
		trimTrailingNewlines(line);
		key = line;
		while (*key && isspace((unsigned char)*key)) key++;
		if (!*key || *key == '#') continue;
		value = strchr(key, '=');
		if (!value) continue;
		*value++ = '\0';
		core_registry_trim(key);
		core_registry_trim(value);
		char bios_system[32];

		if (exactMatch(key, "core_id")) snprintf(core->id, sizeof(core->id), "%s", value);
		else if (exactMatch(key, "display_name")) snprintf(core->name, sizeof(core->name), "%s", value);
		else if (exactMatch(key, "launch_tag")) snprintf(core->launch_tag, sizeof(core->launch_tag), "%s", value);
		else if (exactMatch(key, "libretro_path")) snprintf(core->libretro_path, sizeof(core->libretro_path), "%s", value);
		else if (exactMatch(key, "save_id")) snprintf(core->save_id, sizeof(core->save_id), "%s", value);
		else if (exactMatch(key, "default_priority")) core->default_priority = atoi(value);
		else if (exactMatch(key, "systems")) {
			char tmp[256];
			snprintf(tmp, sizeof(tmp), "%s", value);
			core_registry_split_csv(tmp, core->systems, &core->system_count, CORE_REGISTRY_MAX_CORE_SYSTEMS);
		}
		else if (core_registry_key_with_scope(key, "bios_all", bios_system)) {
			char tmp[256];
			snprintf(tmp, sizeof(tmp), "%s", value);
			core_registry_parse_bios_items(core, tmp, -1, 0, bios_system);
		}
		else if (core_registry_key_with_scope(key, "bios_any", bios_system)) {
			char tmp[256];
			snprintf(tmp, sizeof(tmp), "%s", value);
			core_registry_parse_bios_items(core, tmp, bios_any_group++, 0, bios_system);
		}
		else if (core_registry_key_with_scope(key, "bios_optional_all", bios_system)) {
			char tmp[256];
			snprintf(tmp, sizeof(tmp), "%s", value);
			core_registry_parse_bios_items(core, tmp, -1, 1, bios_system);
		}
		else if (core_registry_key_with_scope(key, "bios_optional_any", bios_system)) {
			char tmp[256];
			snprintf(tmp, sizeof(tmp), "%s", value);
			core_registry_parse_bios_items(core, tmp, bios_any_group++, 1, bios_system);
		}
	}
	fclose(file);

	if (!core->id[0] || !core->launch_tag[0] || !core->libretro_path[0]) return -1;
	if (!core->save_id[0]) snprintf(core->save_id, sizeof(core->save_id), "%s", core->id);
	if (!core->name[0]) snprintf(core->name, sizeof(core->name), "%s", core->id);
	return 0;
}

///////////////////////////////////////
int CORE_REGISTRY_loadRegistry(CoreRegistry* out) {
	FILE* file;
	char line[512];
	DIR* dir;
	struct dirent* de;
	if (!out) return -1;
	memset(out, 0, sizeof(*out));

	file = fopen(CORE_REGISTRY_SYSTEMS_CFG, "r");
	if (file) {
		while (fgets(line, sizeof(line), file) && out->system_count < CORE_REGISTRY_MAX_SYSTEMS) {
			char* id;
			char* name;
			normalizeNewline(line);
			trimTrailingNewlines(line);
			id = line;
			while (*id && isspace((unsigned char)*id)) id++;
			if (!*id || *id == '#') continue;
			name = strchr(id, '|');
			if (name) *name++ = '\0';
			core_registry_trim(id);
			if (name) core_registry_trim(name);
			if (!*id) continue;
			snprintf(out->systems[out->system_count].id, sizeof(out->systems[out->system_count].id), "%s", id);
			snprintf(out->systems[out->system_count].name, sizeof(out->systems[out->system_count].name), "%s", (name && *name) ? name : id);
			out->system_count++;
		}
		fclose(file);
	}

	dir = opendir(CORE_REGISTRY_CORES_DIR);
	if (!dir) return 0;
	while ((de = readdir(dir)) && out->core_count < CORE_REGISTRY_MAX_CORES) {
		char path[MAX_PATH];
		if (de->d_name[0] == '.') continue;
		if (!suffixMatch(".cfg", de->d_name)) continue;
		snprintf(path, sizeof(path), "%s/%s", CORE_REGISTRY_CORES_DIR, de->d_name);
		if (core_registry_parse_core_manifest(path, &out->cores[out->core_count]) == 0) {
			out->core_count++;
		}
	}
	closedir(dir);
	return 0;
}

///////////////////////////////////////
int CORE_REGISTRY_findSystemIndex(const CoreRegistry* reg, const char* system_id) {
	if (!reg || !system_id || !system_id[0]) return -1;
	for (int i=0; i<reg->system_count; i++) {
		if (exactMatch((char*)reg->systems[i].id, (char*)system_id)) return i;
	}
	return -1;
}

int CORE_REGISTRY_coreSupportsSystem(const CoreRegistryCore* core, const char* system_id) {
	if (!core || !system_id || !system_id[0]) return 0;
	for (int i=0; i<core->system_count; i++) {
		if (exactMatch((char*)core->systems[i], (char*)system_id)) return 1;
	}
	return 0;
}

const CoreRegistryCore* CORE_REGISTRY_resolveCore(const CoreRegistry* reg, const char* system_id, const char* override_core_id) {
	const CoreRegistryCore* best = NULL;
	if (!reg || !system_id || !system_id[0]) return NULL;

	if (override_core_id && override_core_id[0]) {
		for (int i=0; i<reg->core_count; i++) {
			const CoreRegistryCore* core = &reg->cores[i];
			if (!exactMatch((char*)core->id, (char*)override_core_id)) continue;
			if (!CORE_REGISTRY_coreSupportsSystem(core, system_id)) continue;
			return core;
		}
	}

	for (int i=0; i<reg->core_count; i++) {
		const CoreRegistryCore* core = &reg->cores[i];
		if (!CORE_REGISTRY_coreSupportsSystem(core, system_id)) continue;
		if (!best ||
			core->default_priority < best->default_priority ||
			(core->default_priority == best->default_priority && strcmp(core->id, best->id) < 0)) {
			best = core;
		}
	}
	return best;
}

const CoreRegistryCore* CORE_REGISTRY_findCoreByLaunchTag(const CoreRegistry* reg, const char* launch_tag) {
	if (!reg || !launch_tag || !launch_tag[0]) return NULL;
	for (int i=0; i<reg->core_count; i++) {
		const CoreRegistryCore* core = &reg->cores[i];
		if (!strcasecmp(core->launch_tag, launch_tag)) return core;
	}
	return NULL;
}

///////////////////////////////////////
static int core_registry_fsync_file(FILE* file) {
	int fd;
	if (!file) return -1;
	if (fflush(file) != 0) return -1;
	fd = fileno(file);
	if (fd < 0) return -1;
	if (fsync(fd) != 0) return -1;
	return 0;
}

static void core_registry_fsync_dir(const char* path) {
	int fd;
	if (!path || !path[0]) return;
	fd = open(path, O_RDONLY | O_DIRECTORY);
	if (fd < 0) return;
	fsync(fd);
	close(fd);
}

int CORE_REGISTRY_getDefaultCoreOverride(const char* system_id, char* core_id, size_t core_id_len) {
	FILE* file;
	char line[256];
	if (!core_id || core_id_len == 0) return -1;
	core_id[0] = '\0';
	if (!system_id || !system_id[0]) return -1;

	file = fopen(CORE_REGISTRY_DEFAULT_CORES_CFG, "r");
	if (!file) return -1;
	while (fgets(line, sizeof(line), file)) {
		char* key;
		char* val;
		normalizeNewline(line);
		trimTrailingNewlines(line);
		key = line;
		while (*key && isspace((unsigned char)*key)) key++;
		if (!*key || *key == '#') continue;
		val = strchr(key, '=');
		if (!val) continue;
		*val++ = '\0';
		core_registry_trim(key);
		core_registry_trim(val);
		if (exactMatch(key, (char*)system_id)) {
			snprintf(core_id, core_id_len, "%s", val);
			fclose(file);
			return 0;
		}
	}
	fclose(file);
	return -1;
}

int CORE_REGISTRY_setDefaultCoreOverride(const char* system_id, const char* core_id) {
	FILE* in;
	FILE* out;
	char line[256];
	char tmp_path[MAX_PATH];
	if (!system_id || !system_id[0]) return -1;

	mkdir(USERDATA_PATH, 0755);
	mkdir(USERDATA_PATH "/emulation", 0755);

	snprintf(tmp_path, sizeof(tmp_path), "%s.tmp", CORE_REGISTRY_DEFAULT_CORES_CFG);
	out = fopen(tmp_path, "w");
	if (!out) return -1;

	in = fopen(CORE_REGISTRY_DEFAULT_CORES_CFG, "r");
	if (in) {
		while (fgets(line, sizeof(line), in)) {
			char keep_line[256];
			char* key;
			char* val;
			snprintf(keep_line, sizeof(keep_line), "%s", line);
			normalizeNewline(line);
			trimTrailingNewlines(line);
			key = line;
			while (*key && isspace((unsigned char)*key)) key++;
			if (!*key || *key == '#') {
				fputs(keep_line, out);
				continue;
			}
			val = strchr(key, '=');
			if (!val) {
				fputs(keep_line, out);
				continue;
			}
			*val++ = '\0';
			core_registry_trim(key);
			core_registry_trim(val);
			if (exactMatch(key, (char*)system_id)) continue;
			fputs(keep_line, out);
		}
		fclose(in);
	}

	if (core_id && core_id[0]) {
		fprintf(out, "%s=%s\n", system_id, core_id);
	}
	if (core_registry_fsync_file(out) != 0) {
		fclose(out);
		unlink(tmp_path);
		return -1;
	}
	if (fclose(out) != 0) {
		unlink(tmp_path);
		return -1;
	}
	if (rename(tmp_path, CORE_REGISTRY_DEFAULT_CORES_CFG) != 0) {
		unlink(tmp_path);
		return -1;
	}
	core_registry_fsync_dir(USERDATA_PATH "/emulation");
	return 0;
}

///////////////////////////////////////
int CORE_REGISTRY_resolveLaunchForSystem(const char* system_name, char* out_core_id, size_t out_core_id_len, char* out_emu_path, size_t out_emu_path_len) {
	CoreRegistry reg;
	const CoreRegistryCore* core;
	char system_id[64];
	char override_core[64] = {0};
	if (out_core_id && out_core_id_len) out_core_id[0] = '\0';
	if (out_emu_path && out_emu_path_len) out_emu_path[0] = '\0';
	if (!system_name || !*system_name) return -1;

	getCanonicalEmuId(system_name, system_id);
	CORE_REGISTRY_loadRegistry(&reg);
	CORE_REGISTRY_getDefaultCoreOverride(system_id, override_core, sizeof(override_core));
	core = CORE_REGISTRY_resolveCore(&reg, system_id, override_core);
	if (!core) return -1;

	if (out_core_id && out_core_id_len) {
		snprintf(out_core_id, out_core_id_len, "%s", core->save_id[0] ? core->save_id : core->id);
	}
	if (out_emu_path && out_emu_path_len) {
		snprintf(out_emu_path, out_emu_path_len, "%s", core->libretro_path);
		if (!exists(out_emu_path)) return -1;
	}
	return 0;
}

int CORE_REGISTRY_resolveSaveIdForSystem(const char* system_name, char* out_save_id, size_t out_save_id_len) {
	CoreRegistry reg;
	const CoreRegistryCore* core;
	char system_id[64];
	char override_core[64] = {0};
	if (!out_save_id || out_save_id_len == 0) return -1;
	out_save_id[0] = '\0';
	if (!system_name || !system_name[0]) return -1;

	getCanonicalEmuId(system_name, system_id);
	CORE_REGISTRY_loadRegistry(&reg);
	CORE_REGISTRY_getDefaultCoreOverride(system_id, override_core, sizeof(override_core));
	core = CORE_REGISTRY_resolveCore(&reg, system_id, override_core);
	if (!core) return -1;

	snprintf(out_save_id, out_save_id_len, "%s", core->save_id[0] ? core->save_id : core->id);
	return 0;
}

int CORE_REGISTRY_resolveSaveIdForLaunchTag(const char* launch_tag, char* out_save_id, size_t out_save_id_len) {
	CoreRegistry reg;
	const CoreRegistryCore* core;
	if (!out_save_id || out_save_id_len == 0) return -1;
	out_save_id[0] = '\0';
	if (!launch_tag || !launch_tag[0]) return -1;

	CORE_REGISTRY_loadRegistry(&reg);
	core = CORE_REGISTRY_findCoreByLaunchTag(&reg, launch_tag);
	if (!core) return -1;
	snprintf(out_save_id, out_save_id_len, "%s", core->save_id[0] ? core->save_id : core->id);
	return 0;
}

///////////////////////////////////////
static void core_registry_add_missing(char missing[][128], int* missing_count, int max_missing, const char* filename) {
	if (!missing || !missing_count || !filename || !filename[0]) return;
	for (int i=0; i<*missing_count; i++) {
		if (exactMatch(missing[i], (char*)filename)) return;
	}
	if (*missing_count >= max_missing) return;
	snprintf(missing[*missing_count], 128, "%s", filename);
	(*missing_count)++;
}

static int core_registry_md5_hex(const char* path, char out_md5[33]) {
	char cmd[768];
	FILE* fp;
	char md5[64] = {0};
	if (!path || !path[0] || !out_md5) return -1;
	snprintf(cmd, sizeof(cmd), "md5sum '%s' 2>/dev/null", path);
	fp = popen(cmd, "r");
	if (!fp) return -1;
	if (fscanf(fp, "%32s", md5) != 1) {
		pclose(fp);
		return -1;
	}
	pclose(fp);
	for (int i=0; i<32; i++) {
		if (!isxdigit((unsigned char)md5[i])) return -1;
		out_md5[i] = (char)tolower((unsigned char)md5[i]);
	}
	out_md5[32] = '\0';
	return 0;
}

static void core_registry_add_bios_root(char roots[][MAX_PATH], int* count, int max_roots, const char* path) {
	if (!roots || !count || max_roots <= 0 || !path || !path[0]) return;
	for (int i=0; i<*count; i++) {
		if (exactMatch(roots[i], (char*)path)) return;
	}
	if (*count >= max_roots) return;
	snprintf(roots[*count], MAX_PATH, "%s", path);
	(*count)++;
}

static int core_registry_collect_bios_roots(const char* bios_root, char roots[][MAX_PATH], int max_roots) {
	const char* merged_bios = getenv("MINUI_BIOS_DIR");
	int count = 0;
	if (!merged_bios || !merged_bios[0]) {
		merged_bios = getenv("BIOS_PATH");
	}
	if (!roots || max_roots <= 0) return 0;
	core_registry_add_bios_root(roots, &count, max_roots, bios_root);
	core_registry_add_bios_root(roots, &count, max_roots, merged_bios);
	core_registry_add_bios_root(roots, &count, max_roots, SDCARD_PATH "/bios");
	core_registry_add_bios_root(roots, &count, max_roots, SDCARD2_PATH "/bios");
	return count;
}

static int core_registry_bios_rule_ok(const CoreRegistryBiosRule* rule, char roots[][MAX_PATH], int root_count) {
	char path[MAX_PATH];
	char actual_md5[33] = {0};
	if (!rule || !roots || root_count <= 0 || !rule->filename[0]) return 0;
	for (int i=0; i<root_count; i++) {
		snprintf(path, sizeof(path), "%s/%s", roots[i], rule->filename);
		if (!exists(path)) continue;
		if (!rule->md5[0]) return 1;
		if (core_registry_md5_hex(path, actual_md5) != 0) continue;
		if (exactMatch(actual_md5, (char*)rule->md5)) return 1;
	}
	return 0;
}

static int core_registry_bios_rule_applies(const CoreRegistryBiosRule* rule, const char* system_id) {
	if (!rule) return 0;
	if (!rule->system_id[0]) return 1;
	if (!system_id || !system_id[0]) return 0;
	return exactMatch((char*)rule->system_id, (char*)system_id);
}

int CORE_REGISTRY_checkBios(const CoreRegistryCore* core, const char* system_id, const char* bios_root, char missing[][128], int max_missing) {
	int missing_count = 0;
	int groups[CORE_REGISTRY_MAX_BIOS_RULES];
	int group_count = 0;
	char roots[CORE_REGISTRY_MAX_BIOS_ROOTS][MAX_PATH];
	int root_count = 0;
	if (!core || !missing || max_missing <= 0) return 0;

	root_count = core_registry_collect_bios_roots(bios_root, roots, CORE_REGISTRY_MAX_BIOS_ROOTS);
	if (root_count <= 0) return 0;

	for (int i=0; i<core->bios_rule_count; i++) {
		const CoreRegistryBiosRule* rule = &core->bios_rules[i];
		if (!core_registry_bios_rule_applies(rule, system_id)) continue;
		if (rule->group != -1 || rule->optional) continue;
		if (!core_registry_bios_rule_ok(rule, roots, root_count)) {
			core_registry_add_missing(missing, &missing_count, max_missing, rule->filename);
		}
	}

	for (int i=0; i<core->bios_rule_count; i++) {
		int g = core->bios_rules[i].group;
		int seen = 0;
		if (!core_registry_bios_rule_applies(&core->bios_rules[i], system_id)) continue;
		if (g < 0) continue;
		if (core->bios_rules[i].optional) continue;
		for (int j=0; j<group_count; j++) {
			if (groups[j] == g) {
				seen = 1;
				break;
			}
		}
		if (!seen && group_count < CORE_REGISTRY_MAX_BIOS_RULES) groups[group_count++] = g;
	}

	for (int gi=0; gi<group_count; gi++) {
		int group_ok = 0;
		int g = groups[gi];
		for (int i=0; i<core->bios_rule_count; i++) {
			const CoreRegistryBiosRule* rule = &core->bios_rules[i];
			if (!core_registry_bios_rule_applies(rule, system_id)) continue;
			if (rule->group != g) continue;
			if (rule->optional) continue;
			if (core_registry_bios_rule_ok(rule, roots, root_count)) {
				group_ok = 1;
				break;
			}
		}
		if (group_ok) continue;
		for (int i=0; i<core->bios_rule_count; i++) {
			const CoreRegistryBiosRule* rule = &core->bios_rules[i];
			if (!core_registry_bios_rule_applies(rule, system_id)) continue;
			if (rule->group != g) continue;
			if (rule->optional) continue;
			core_registry_add_missing(missing, &missing_count, max_missing, rule->filename);
		}
	}
	return missing_count;
}

static int core_registry_find_bios_file_status(CoreRegistryBiosFileStatus* out, int count, const char* filename) {
	if (!out || !filename || !filename[0] || count <= 0) return -1;
	for (int i=0; i<count; i++) {
		if (exactMatch(out[i].filename, (char*)filename)) return i;
	}
	return -1;
}

int CORE_REGISTRY_listBiosFiles(const CoreRegistryCore* core, const char* system_id, const char* bios_root, CoreRegistryBiosFileStatus* out, int max_out) {
	char roots[CORE_REGISTRY_MAX_BIOS_ROOTS][MAX_PATH];
	int root_count = 0;
	int count = 0;
	if (!core || !out || max_out <= 0) return 0;

	memset(out, 0, sizeof(*out) * max_out);
	root_count = core_registry_collect_bios_roots(bios_root, roots, CORE_REGISTRY_MAX_BIOS_ROOTS);

	for (int i=0; i<core->bios_rule_count; i++) {
		const CoreRegistryBiosRule* rule = &core->bios_rules[i];
		int idx;
		if (!core_registry_bios_rule_applies(rule, system_id)) continue;
		if (!rule->filename[0]) continue;
		idx = core_registry_find_bios_file_status(out, count, rule->filename);
		if (idx < 0) {
			if (count >= max_out) continue;
			idx = count++;
			snprintf(out[idx].filename, sizeof(out[idx].filename), "%s", rule->filename);
			out[idx].optional = rule->optional ? 1 : 0;
			out[idx].present = 0;
		}
		else if (!rule->optional) {
			out[idx].optional = 0;
		}
		if (root_count > 0 && core_registry_bios_rule_ok(rule, roots, root_count)) {
			out[idx].present = 1;
		}
	}
	return count;
}
