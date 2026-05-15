#include "minarch.h"

///////////////////////////////////////

static struct {
	int palette_updated;
} special;

static void Special_updatedDMGPalette(int frames)
{
	special.palette_updated = frames;
}

static void Special_refreshDMGPalette(void)
{
	special.palette_updated -= 1;
	if (special.palette_updated > 0)
		return;

	int rgb = getInt("/tmp/dmg_grid_color");
	GFX_setEffectColor(rgb);
}

void Special_init(void)
{
	if (special.palette_updated > 1)
		special.palette_updated = 1;
}

void Special_render(void)
{
	if (special.palette_updated)
		Special_refreshDMGPalette();
}

void Special_quit(void)
{
	system("rm -f /tmp/dmg_grid_color");
}

///////////////////////////////////////

static int Option_getValueIndex(Option *item, const char *value)
{
	if (!value)
		return 0;
	for (int i = 0; i < item->count; i++) {
		if (!strcmp(item->values[i], value))
			return i;
	}
	return 0;
}

static void Option_setValue(Option *item, const char *value)
{
	item->value = Option_getValueIndex(item, value);
}

static const char *option_key_name[] = {
	"pcsx_rearmed_analog_combo", "DualShock Toggle Combo",
	NULL
};

static const char *getOptionNameFromKey(const char *key, const char *name)
{
	char *_key = NULL;

	for (int i = 0; (_key = (char *)option_key_name[i]); i += 2) {
		if (exactMatch((char *)key, _key))
			return option_key_name[i + 1];
	}
	return name;
}

void OptionList_init(const struct retro_core_option_definition *defs)
{
	LOG_info("OptionList_init\n");
	int count;

	for (count = 0; defs[count].key; count++)
		;

	config.core.count = count;
	if (!count)
		return;

	config.core.options = calloc(count + 1, sizeof(Option));
	for (int i = 0; i < config.core.count; i++) {
		int len;
		const struct retro_core_option_definition *def = &defs[i];
		Option *item = &config.core.options[i];

		len = strlen(def->key) + 1;
		item->key = calloc(len, sizeof(char));
		strcpy(item->key, def->key);

		len = strlen(def->desc) + 1;
		item->name = calloc(len, sizeof(char));
		strcpy(item->name, getOptionNameFromKey(def->key, def->desc));

		if (def->info) {
			len = strlen(def->info) + 1;
			item->desc = calloc(len, sizeof(char));
			strncpy(item->desc, def->info, len);

			item->full = calloc(len, sizeof(char));
			strncpy(item->full, item->desc, len);

			GFX_wrapText(font.tiny, item->desc, SCALE1(240), 2);
			GFX_wrapText(font.medium, item->full, SCALE1(240), 7);
		}

		for (count = 0; def->values[count].value; count++)
			;

		item->count = count;
		item->values = calloc(count + 1, sizeof(char *));
		item->labels = calloc(count + 1, sizeof(char *));

		for (int j = 0; j < count; j++) {
			const char *value = def->values[j].value;
			const char *label = def->values[j].label;

			len = strlen(value) + 1;
			item->values[j] = calloc(len, sizeof(char));
			strcpy(item->values[j], value);

			if (label) {
				len = strlen(label) + 1;
				item->labels[j] = calloc(len, sizeof(char));
				strcpy(item->labels[j], label);
			} else {
				item->labels[j] = item->values[j];
			}
		}

		item->value = Option_getValueIndex(item, def->default_value);
		item->default_value = item->value;
	}
}

void OptionList_vars(const struct retro_variable *vars)
{
	LOG_info("OptionList_vars\n");
	int count;

	for (count = 0; vars[count].key; count++)
		;

	config.core.count = count;
	if (!count)
		return;

	config.core.options = calloc(count + 1, sizeof(Option));
	for (int i = 0; i < config.core.count; i++) {
		int len;
		const struct retro_variable *var = &vars[i];
		Option *item = &config.core.options[i];
		char *tmp;

		len = strlen(var->key) + 1;
		item->key = calloc(len, sizeof(char));
		strcpy(item->key, var->key);

		len = strlen(var->value) + 1;
		item->var = calloc(len, sizeof(char));
		strcpy(item->var, var->value);

		tmp = strchr(item->var, ';');
		if (tmp && *(tmp + 1) == ' ') {
			*tmp = '\0';
			item->name = item->var;
			tmp += 2;
		}

		char *opt = tmp;
		for (count = 0; (tmp = strchr(tmp, '|')); tmp++, count++)
			;
		count += 1;

		item->count = count;
		item->values = calloc(count + 1, sizeof(char *));
		item->labels = calloc(count + 1, sizeof(char *));

		tmp = opt;
		int j;
		for (j = 0; (tmp = strchr(tmp, '|')); j++) {
			item->values[j] = opt;
			item->labels[j] = opt;
			*tmp = '\0';
			tmp += 1;
			opt = tmp;
		}
		item->values[j] = opt;
		item->labels[j] = opt;
		item->value = 0;
		item->default_value = item->value;
	}
}

void OptionList_reset(void)
{
	if (!config.core.count)
		return;

	for (int i = 0; i < config.core.count; i++) {
		Option *item = &config.core.options[i];
		if (item->var) {
			free(item->var);
		} else {
			if (item->desc)
				free(item->desc);
			if (item->full)
				free(item->full);
			for (int j = 0; j < item->count; j++) {
				char *value = item->values[j];
				char *label = item->labels[j];
				if (label != value)
					free(label);
				free(value);
			}
		}
		free(item->values);
		free(item->labels);
		free(item->key);
		free(item->name);
	}
	if (config.core.enabled_options)
		free(config.core.enabled_options);
	config.core.enabled_count = 0;
	free(config.core.options);
}

Option *OptionList_getOption(OptionList *list, const char *key)
{
	for (int i = 0; i < list->count; i++) {
		Option *item = &list->options[i];
		if (!strcmp(item->key, key))
			return item;
	}
	return NULL;
}

char *OptionList_getOptionValue(OptionList *list, const char *key)
{
	Option *item = OptionList_getOption(list, key);

	if (item)
		return item->values[item->value];
	LOG_warn("unknown option %s \n", key);
	return NULL;
}

void OptionList_setOptionRawValue(OptionList *list, const char *key, int value)
{
	Option *item = OptionList_getOption(list, key);

	if (item) {
		item->value = value;
		list->changed = 1;
		if (exactMatch(core.tag, "GB") &&
			containsString(item->key, "palette")) {
			Special_updatedDMGPalette(3);
		}
	} else {
		LOG_info("unknown option %s \n", key);
	}
}

void OptionList_setOptionValue(OptionList *list, const char *key,
	const char *value)
{
	Option *item = OptionList_getOption(list, key);

	if (item) {
		Option_setValue(item, value);
		list->changed = 1;
		if (exactMatch(core.tag, "GB") &&
			containsString(item->key, "palette")) {
			Special_updatedDMGPalette(2);
		}
	} else {
		LOG_info("unknown option %s \n", key);
	}
}
