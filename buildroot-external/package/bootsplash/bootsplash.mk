################################################################################
# bootsplash
################################################################################

BOOTSPLASH_VERSION = local
BOOTSPLASH_SITE = $(BR2_EXTERNAL_ALLIUM_EXTERNAL_PATH)/package/bootsplash
BOOTSPLASH_SITE_METHOD = local
BOOTSPLASH_LICENSE = See upstream

BOOTSPLASH_SRC_DIR = $(@D)/src
BOOTSPLASH_SCRIPT_DIR = $(@D)/scripts
BOOTSPLASH_SPLASH_BMP = \
	$(BR2_EXTERNAL_ALLIUM_EXTERNAL_PATH)/board/rg35xxsp/splash.bmp
BOOTSPLASH_SPLASH_FB = $(@D)/build-bootsplash/splash.fb

# Tiny fbdev utility: no SDL/DBus dependencies, safe for early boot usage.
define BOOTSPLASH_BUILD_CMDS
	mkdir -p $(@D)/build-bootsplash
	$(TARGET_CC) $(TARGET_CFLAGS) -std=gnu99 \
		$(BOOTSPLASH_SRC_DIR)/bootsplash.c \
		-o $(@D)/build-bootsplash/bootsplash \
		$(TARGET_LDFLAGS)
	if [ -f "$(BOOTSPLASH_SPLASH_BMP)" ]; then \
		python3 $(BOOTSPLASH_SCRIPT_DIR)/bmp_to_fb.py \
			"$(BOOTSPLASH_SPLASH_BMP)" \
			"$(BOOTSPLASH_SPLASH_FB)"; \
	fi
endef

define BOOTSPLASH_INSTALL_TARGET_CMDS
	$(INSTALL) -D -m 0755 $(@D)/build-bootsplash/bootsplash \
		$(TARGET_DIR)/usr/bin/bootsplash
	if [ -f "$(BOOTSPLASH_SPLASH_FB)" ]; then \
		$(INSTALL) -D -m 0644 "$(BOOTSPLASH_SPLASH_FB)" \
			$(TARGET_DIR)/usr/share/bootsplash/splash.fb; \
	fi
endef

$(eval $(generic-package))
