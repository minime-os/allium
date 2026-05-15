#define _GNU_SOURCE // for strcasestr
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <fcntl.h>
#include <math.h>
#include <ctype.h>
#include <sys/time.h>
#include "defines.h"
#include "core_registry.h"
#include "utils.h"

///////////////////////////////////////

int prefixMatch(char* pre, char* str) {
	if (!pre || !str) {
		return 0;
	}
	return (strncasecmp(pre,str,strlen(pre))==0);
}
int suffixMatch(char* suf, char* str) {
	int len = strlen(suf);
	int offset = strlen(str)-len;
	return (offset>=0 && strncasecmp(suf, str+offset, len)==0);
}
int exactMatch(char* str1, char* str2) {
	if (!str1 || !str2) return 0; // NULL isn't safe here
	int len1 = strlen(str1);
	if (len1!=strlen(str2)) return 0;
	return (strncmp(str1,str2,len1)==0);
}
int containsString(char* haystack, char* needle) {
	return strcasestr(haystack, needle) != NULL;
}
int hide(char* file_name) {
	return file_name[0]=='.' || suffixMatch(".disabled", file_name) || exactMatch("map.txt", file_name);
}

void getDisplayName(const char* in_name, char* out_name) {
	char* tmp;
	char work_name[256];
	strcpy(work_name, in_name);
	strcpy(out_name, in_name);
	
	if (suffixMatch("/" PLATFORM, work_name)) { // hide platform from Tools path...
		tmp = strrchr(work_name, '/');
		tmp[0] = '\0';
	}
	
	// extract just the filename if necessary
	tmp = strrchr(work_name, '/');
	if (tmp) strcpy(out_name, tmp+1);
	
	// remove extension(s), eg. .p8.png
	while ((tmp = strrchr(out_name, '.'))!=NULL) {
		int len = strlen(tmp);
		if (len>2 && len<=5) tmp[0] = '\0'; // 1-4 letter extension plus dot (was 1-3, extended for .doom files)
		else break;
	}
	
	// remove trailing parens (round and square)
	strcpy(work_name, out_name);
	while ((tmp=strrchr(out_name, '('))!=NULL || (tmp=strrchr(out_name, '['))!=NULL) {
		if (tmp==out_name) break;
		tmp[0] = '\0';
		tmp = out_name;
	}
	
	// make sure we haven't nuked the entire name
	if (out_name[0]=='\0') strcpy(out_name, work_name);
	
	// remove trailing whitespace
	tmp = out_name + strlen(out_name) - 1;
    while(tmp>out_name && isspace((unsigned char)*tmp)) tmp--;
    tmp[1] = '\0';
}
void getEmuName(const char* in_name, char* out_name) { // NOTE: both char arrays need to be MAX_PATH length!
	char* tmp;
	strcpy(out_name, in_name);
	tmp = out_name;
	
	// printf("--------\n  in_name: %s\n",in_name); fflush(stdout);
	
	// extract just the Roms folder name if necessary
	if (prefixMatch(ROMS_PATH, tmp) || prefixMatch(ROMS2_PATH, tmp)) {
		if (prefixMatch(ROMS_PATH, tmp)) tmp += strlen(ROMS_PATH) + 1;
		else tmp += strlen(ROMS2_PATH) + 1;
		char* tmp2 = strchr(tmp, '/');
		if (tmp2) tmp2[0] = '\0';
		strcpy(out_name, tmp);
		tmp = out_name;
	}

	// finally extract the short launch tag from parentheses if present
	tmp = strrchr(tmp, '(');
	if (tmp) {
		tmp += 1;
		// printf("    tmp2: %s\n", tmp);
		strcpy(out_name, tmp);
		tmp = strchr(out_name,')');
		tmp[0] = '\0';
	}
	
}
void getCanonicalEmuId(const char* in_name, char* out_name) {
	strcpy(out_name, in_name);

	if (!strcasecmp(out_name, "PAK") || !strcasecmp(out_name, "PORTS")) strcpy(out_name, "ports");
	else if (!strcasecmp(out_name, "FC") || !strcasecmp(out_name, "NES")) strcpy(out_name, "nes");
	else if (!strcasecmp(out_name, "GB")) strcpy(out_name, "gb");
	else if (!strcasecmp(out_name, "GBC")) strcpy(out_name, "gbc");
	else if (!strcasecmp(out_name, "GBA")) strcpy(out_name, "gba");
	else if (!strcasecmp(out_name, "MGBA")) strcpy(out_name, "mgba");
	else if (!strcasecmp(out_name, "SGB")) strcpy(out_name, "sgb");
	else if (!strcasecmp(out_name, "GG") || !strcasecmp(out_name, "GAMEGEAR")) strcpy(out_name, "gamegear");
	else if (!strcasecmp(out_name, "MD") || !strcasecmp(out_name, "GENESIS")) strcpy(out_name, "genesis");
	else if (!strcasecmp(out_name, "SMS") || !strcasecmp(out_name, "MS") || !strcasecmp(out_name, "MASTERSYSTEM")) strcpy(out_name, "mastersystem");
	else if (!strcasecmp(out_name, "NGP")) strcpy(out_name, "ngp");
	else if (!strcasecmp(out_name, "NGPC")) strcpy(out_name, "ngpc");
	else if (!strcasecmp(out_name, "PCE") || !strcasecmp(out_name, "TG16")) strcpy(out_name, "tg16");
	else if (!strcasecmp(out_name, "PS") || !strcasecmp(out_name, "PSX")) strcpy(out_name, "psx");
	else if (!strcasecmp(out_name, "SFC") || !strcasecmp(out_name, "SNES")) strcpy(out_name, "snes");
	else if (!strcasecmp(out_name, "SUPA")) strcpy(out_name, "supa");
	else if (!strcasecmp(out_name, "VB") || !strcasecmp(out_name, "VIRTUALBOY")) strcpy(out_name, "virtualboy");
	else if (!strcasecmp(out_name, "PKM") || !strcasecmp(out_name, "POKEMINI")) strcpy(out_name, "pokemini");
	else if (!strcasecmp(out_name, "P8") || !strcasecmp(out_name, "PICO8")) strcpy(out_name, "pico8");
	else {
		char* tmp = out_name;
		while (*tmp) {
			*tmp = tolower((unsigned char)*tmp);
			tmp += 1;
		}
	}
}
void getEmuPath(char* core_registry_name, char* core_path) {
	char system_id[64];
	getCanonicalEmuId(core_registry_name, system_id);
	if (CORE_REGISTRY_resolveLaunchForSystem(system_id, NULL, 0, core_path, MAX_PATH) == 0) return;
	core_path[0] = '\0';
}

void normalizeNewline(char* line) {
	int len = strlen(line);
	if (len>1 && line[len-1]=='\n' && line[len-2]=='\r') { // windows!
		line[len-2] = '\n';
		line[len-1] = '\0';
	}
}
void trimTrailingNewlines(char* line) {
	int len = strlen(line);
	while (len>0 && line[len-1]=='\n') {
		line[len-1] = '\0'; // trim newline
		len -= 1;
	}
}
void trimSortingMeta(char** str) { // eg. `001) `
	// TODO: this code is suss
	char* safe = *str;
	while(isdigit(**str)) *str += 1; // ignore leading numbers

	if (*str[0]==')') { // then match a closing parenthesis
		*str += 1;
	}
	else { //  or bail, restoring the string to its original value
		*str = safe;
		return;
	}
	
	while(isblank(**str)) *str += 1; // ignore leading space
}

///////////////////////////////////////

int exists(char* path) {
	return access(path, F_OK)==0;
}
void touch(char* path) {
	close(open(path, O_RDWR|O_CREAT, 0777));
}
void putFile(char* path, char* contents) {
	FILE* file = fopen(path, "w");
	if (file) {
		fputs(contents, file);
		fclose(file);
	}
}
void getFile(char* path, char* buffer, size_t buffer_size) {
	FILE *file = fopen(path, "r");
	if (file) {
		fseek(file, 0L, SEEK_END);
		size_t size = ftell(file);
		if (size>buffer_size-1) size = buffer_size - 1;
		rewind(file);
		fread(buffer, sizeof(char), size, file);
		fclose(file);
		buffer[size] = '\0';
	}
}
char* allocFile(char* path) { // caller must free!
	char* contents = NULL;
	FILE *file = fopen(path, "r");
	if (file) {
		fseek(file, 0L, SEEK_END);
		size_t size = ftell(file);
		contents = calloc(size+1, sizeof(char));
		fseek(file, 0L, SEEK_SET);
		fread(contents, sizeof(char), size, file);
		fclose(file);
		contents[size] = '\0';
	}
	return contents;
}
int getInt(char* path) {
	int i = 0;
	FILE *file = fopen(path, "r");
	if (file!=NULL) {
		fscanf(file, "%i", &i);
		fclose(file);
	}
	return i;
}
void putInt(char* path, int value) {
	char buffer[8];
	sprintf(buffer, "%d", value);
	putFile(path, buffer);
}

uint64_t getMicroseconds(void) {
    uint64_t ret;
    struct timeval tv;

    gettimeofday(&tv, NULL);

    ret = (uint64_t)tv.tv_sec * 1000000;
    ret += (uint64_t)tv.tv_usec;

    return ret;
}
