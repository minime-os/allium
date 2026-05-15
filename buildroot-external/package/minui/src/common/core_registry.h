#ifndef CORE_REGISTRY_H
#define CORE_REGISTRY_H

#include <stddef.h>

#define CORE_REGISTRY_MAX_SYSTEMS 64
#define CORE_REGISTRY_MAX_CORES 64
#define CORE_REGISTRY_MAX_CORE_SYSTEMS 16
#define CORE_REGISTRY_MAX_BIOS_RULES 64
#define CORE_REGISTRY_MAX_MISSING_BIOS 64
#define CORE_REGISTRY_MAX_BIOS_ROOTS 4

typedef struct CoreRegistrySystem {
	char id[32];
	char name[96];
} CoreRegistrySystem;

typedef struct CoreRegistryBiosRule {
	char filename[128];
	char md5[33];
	char system_id[32]; // empty => applies to all systems supported by core
	int group; // -1 => all_of, >=0 => any_of group id
	int optional; // 0 => required, 1 => optional
} CoreRegistryBiosRule;

typedef struct CoreRegistryCore {
	char id[32];
	char name[64];
	char launch_tag[32];
	char save_id[32];
	char libretro_path[MAX_PATH];
	int default_priority;
	int system_count;
	char systems[CORE_REGISTRY_MAX_CORE_SYSTEMS][32];
	int bios_rule_count;
	CoreRegistryBiosRule bios_rules[CORE_REGISTRY_MAX_BIOS_RULES];
} CoreRegistryCore;

typedef struct CoreRegistry {
	int system_count;
	CoreRegistrySystem systems[CORE_REGISTRY_MAX_SYSTEMS];
	int core_count;
	CoreRegistryCore cores[CORE_REGISTRY_MAX_CORES];
} CoreRegistry;

typedef struct CoreRegistryBiosFileStatus {
	char filename[128];
	int optional;
	int present;
} CoreRegistryBiosFileStatus;

///////////////////////////////////////
int CORE_REGISTRY_loadRegistry(CoreRegistry* out);
int CORE_REGISTRY_findSystemIndex(const CoreRegistry* reg, const char* system_id);
int CORE_REGISTRY_coreSupportsSystem(const CoreRegistryCore* core, const char* system_id);
const CoreRegistryCore* CORE_REGISTRY_resolveCore(const CoreRegistry* reg, const char* system_id, const char* override_core_id);
const CoreRegistryCore* CORE_REGISTRY_findCoreByLaunchTag(const CoreRegistry* reg, const char* launch_tag);

///////////////////////////////////////
int CORE_REGISTRY_getDefaultCoreOverride(const char* system_id, char* core_id, size_t core_id_len);
int CORE_REGISTRY_setDefaultCoreOverride(const char* system_id, const char* core_id);
int CORE_REGISTRY_resolveLaunchForSystem(const char* system_id, char* out_core_id, size_t out_core_id_len, char* out_emu_path, size_t out_emu_path_len);
int CORE_REGISTRY_resolveSaveIdForSystem(const char* system_id, char* out_save_id, size_t out_save_id_len);
int CORE_REGISTRY_resolveSaveIdForLaunchTag(const char* launch_tag, char* out_save_id, size_t out_save_id_len);

///////////////////////////////////////
int CORE_REGISTRY_checkBios(const CoreRegistryCore* core, const char* system_id, const char* bios_root, char missing[][128], int max_missing);
int CORE_REGISTRY_listBiosFiles(const CoreRegistryCore* core, const char* system_id, const char* bios_root, CoreRegistryBiosFileStatus* out, int max_out);

#endif
