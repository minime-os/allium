################################################################################
# minui
################################################################################

MINUI_VERSION = local
MINUI_SITE = $(BR2_EXTERNAL_ALLIUM_EXTERNAL_PATH)/package/minui
MINUI_SITE_METHOD = local
MINUI_LICENSE = See upstream
MINUI_DEPENDENCIES = dbus libretro-common lz4 sdl2 sdl2_image sdl2_ttf zlib

MINUI_SHARE_DIR = /usr/share/minui
MINUI_LIBEXEC_DIR = /usr/libexec/minui
MINUI_LIB_DIR = /usr/lib/minui
MINUI_CORE_LIB_DIR = $(MINUI_LIB_DIR)/cores
MINUI_RUNTIME_RPATH = -Wl,-rpath,$(MINUI_LIB_DIR)

MINUI_SRC_DIR = $(@D)/src
MINUI_ASSETS_DIR = $(@D)/assets
MINUI_BUILD_DIR = $(@D)/build-minui
MINUI_PLATFORM_DIR = $(MINUI_SRC_DIR)/platform/rg35xxplus
MINUI_PLATFORM_NAME = rg35xxplus
MINUI_TIMEZONE_SRC = $(MINUI_ASSETS_DIR)/timezones/minui.tzs
MINUI_ZIC = $(shell command -v zic 2>/dev/null || echo /usr/sbin/zic)
MINUI_BUILD_DATE = $(shell date +%Y.%m.%d)
MINUI_BUILD_HASH = $(shell git -C $(BR2_EXTERNAL_ALLIUM_EXTERNAL_PATH)/.. rev-parse --short HEAD 2>/dev/null || echo clean-start)
MINUI_DBUS_CFLAGS = -I$(STAGING_DIR)/usr/include/dbus-1.0 \
	-I$(STAGING_DIR)/usr/lib/dbus-1.0/include
MINUI_LIBRETRO_CFLAGS = -I$(STAGING_DIR)/usr/include
MINUI_LZ4_CFLAGS = -I$(BASE_DIR)/host/include
MINUI_LZ4_LDFLAGS = -L$(BASE_DIR)/per-package/lz4/host/aarch64-buildroot-linux-musl/sysroot/usr/lib
MINUI_CPPFLAGS = -DPLATFORM=\"$(MINUI_PLATFORM_NAME)\"

define MINUI_BUILD_CMDS
	mkdir -p $(MINUI_BUILD_DIR)
	$(TARGET_CC) $(TARGET_CFLAGS) -fPIC \
		-I$(MINUI_SRC_DIR)/libmsettings \
		-c $(MINUI_SRC_DIR)/libmsettings/msettings.c \
		-o $(MINUI_BUILD_DIR)/msettings.o
	$(TARGET_CC) $(TARGET_LDFLAGS) -shared -Wl,-soname,libmsettings.so \
		-o $(MINUI_BUILD_DIR)/libmsettings.so \
		$(MINUI_BUILD_DIR)/msettings.o -ldl -lrt
	$(TARGET_CC) $(TARGET_CFLAGS) \
		-I$(MINUI_SRC_DIR)/common \
		-I$(MINUI_SRC_DIR)/libmsettings \
		-I$(MINUI_PLATFORM_DIR) \
		$(MINUI_SRC_DIR)/keymon/keymon.c \
		-o $(MINUI_BUILD_DIR)/keymon \
		$(TARGET_LDFLAGS) $(MINUI_RUNTIME_RPATH) -L$(MINUI_BUILD_DIR) -lmsettings -lpthread -lrt -ldl
	$(TARGET_CC) $(TARGET_CFLAGS) $(MINUI_CPPFLAGS) -fomit-frame-pointer -std=gnu99 \
		$(MINUI_LIBRETRO_CFLAGS) \
		$(MINUI_LZ4_CFLAGS) \
		-I$(MINUI_SRC_DIR)/minarch \
		-I$(MINUI_SRC_DIR)/common \
		-I$(MINUI_SRC_DIR)/libmsettings \
		-I$(MINUI_PLATFORM_DIR) \
		-DUSE_SDL2 \
		-DBUILD_DATE=\"$(MINUI_BUILD_DATE)\" -DBUILD_HASH=\"$(MINUI_BUILD_HASH)\" \
		$(MINUI_SRC_DIR)/minarch/main.c \
		$(MINUI_SRC_DIR)/minarch/core.c \
		$(MINUI_SRC_DIR)/minarch/content.c \
		$(MINUI_SRC_DIR)/minarch/config.c \
		$(MINUI_SRC_DIR)/minarch/options.c \
		$(MINUI_SRC_DIR)/minarch/input.c \
		$(MINUI_SRC_DIR)/minarch/rewind.c \
		$(MINUI_SRC_DIR)/minarch/video.c \
		$(MINUI_SRC_DIR)/minarch/menu.c \
		$(MINUI_SRC_DIR)/common/scaler.c \
		$(MINUI_SRC_DIR)/common/utils.c \
		$(MINUI_SRC_DIR)/common/api.c \
		$(MINUI_SRC_DIR)/common/core_registry.c \
		$(MINUI_PLATFORM_DIR)/platform.c \
		-o $(MINUI_BUILD_DIR)/minarch \
		$(TARGET_LDFLAGS) $(MINUI_RUNTIME_RPATH) $(MINUI_LZ4_LDFLAGS) -L$(MINUI_BUILD_DIR) -ldl -llz4 -lmsettings -lSDL2 -lSDL2_image -lSDL2_ttf -lpthread -lm -lz
	$(TARGET_CC) $(TARGET_CFLAGS) $(MINUI_CPPFLAGS) -fomit-frame-pointer -std=gnu99 \
		$(MINUI_DBUS_CFLAGS) \
		-I$(MINUI_SRC_DIR)/main \
		-I$(MINUI_SRC_DIR)/settings \
		-I$(MINUI_SRC_DIR)/ui \
		-I$(MINUI_SRC_DIR)/common \
		-I$(MINUI_SRC_DIR)/libmsettings \
		-I$(MINUI_PLATFORM_DIR) \
		-DUSE_SDL2 \
		$(MINUI_SRC_DIR)/main/main.c \
		$(MINUI_SRC_DIR)/settings/settings.c \
		$(MINUI_SRC_DIR)/settings/menu.c \
		$(MINUI_SRC_DIR)/settings/jobs.c \
		$(MINUI_SRC_DIR)/settings/timezone.c \
		$(MINUI_SRC_DIR)/settings/wifi_backend.c \
		$(MINUI_SRC_DIR)/settings/bt_backend.c \
		$(MINUI_SRC_DIR)/settings/about.c \
		$(MINUI_SRC_DIR)/settings/power.c \
		$(MINUI_SRC_DIR)/settings/time.c \
		$(MINUI_SRC_DIR)/settings/wifi.c \
		$(MINUI_SRC_DIR)/settings/bt.c \
		$(MINUI_SRC_DIR)/settings/controls.c \
		$(MINUI_SRC_DIR)/ui/badge.c \
		$(MINUI_SRC_DIR)/ui/list.c \
		$(MINUI_SRC_DIR)/ui/dialog.c \
		$(MINUI_SRC_DIR)/ui/keyboard.c \
		$(MINUI_SRC_DIR)/common/scaler.c \
		$(MINUI_SRC_DIR)/common/utils.c \
		$(MINUI_SRC_DIR)/common/api.c \
		$(MINUI_SRC_DIR)/common/core_registry.c \
		$(MINUI_PLATFORM_DIR)/platform.c \
		-o $(MINUI_BUILD_DIR)/minui \
		$(TARGET_LDFLAGS) $(MINUI_RUNTIME_RPATH) -L$(MINUI_BUILD_DIR) -ldbus-1 -ldl -lmsettings -lSDL2 -lSDL2_image -lSDL2_ttf -lpthread -lm -lz
	$(TARGET_CC) $(TARGET_CFLAGS) \
		$(MINUI_SRC_DIR)/show/show.c \
		-o $(MINUI_BUILD_DIR)/minui-show \
		$(TARGET_LDFLAGS) -lSDL2 -lSDL2_image -lrt -ldl
endef

define MINUI_INSTALL_TARGET_CMDS
	$(INSTALL) -D -m 0755 $(MINUI_BUILD_DIR)/minui \
		$(TARGET_DIR)/usr/bin/minui
	$(INSTALL) -D -m 0755 $(MINUI_BUILD_DIR)/minarch \
		$(TARGET_DIR)/usr/bin/minarch
	$(INSTALL) -D -m 0755 $(MINUI_BUILD_DIR)/minui-show \
		$(TARGET_DIR)/usr/bin/minui-show
	$(INSTALL) -D -m 0755 $(MINUI_BUILD_DIR)/keymon \
		$(TARGET_DIR)$(MINUI_LIBEXEC_DIR)/keymon
	$(INSTALL) -D -m 0755 $(MINUI_ASSETS_DIR)/minui-session.sh \
		$(TARGET_DIR)$(MINUI_LIBEXEC_DIR)/session
	$(INSTALL) -D -m 0755 $(MINUI_ASSETS_DIR)/etc/wifi/wifi_init.sh \
		$(TARGET_DIR)$(MINUI_LIBEXEC_DIR)/wifi-init.sh
	$(INSTALL) -D -m 0755 $(MINUI_ASSETS_DIR)/etc/bluetooth/bt_init.sh \
		$(TARGET_DIR)$(MINUI_LIBEXEC_DIR)/bluetooth-init.sh
	if [ -f "$(MINUI_ASSETS_DIR)/bin/hdmimon.sh" ]; then \
		$(INSTALL) -D -m 0755 $(MINUI_ASSETS_DIR)/bin/hdmimon.sh \
			$(TARGET_DIR)$(MINUI_LIBEXEC_DIR)/hdmimon.sh; \
	fi
	$(INSTALL) -D -m 0755 $(MINUI_ASSETS_DIR)/S16minui \
		$(TARGET_DIR)/etc/init.d/S16minui
	$(INSTALL) -D -m 0755 $(MINUI_BUILD_DIR)/libmsettings.so \
		$(TARGET_DIR)$(MINUI_LIB_DIR)/libmsettings.so
	ln -sf ../lib/minui/libmsettings.so $(TARGET_DIR)/usr/lib/libmsettings.so
	mkdir -p \
		$(TARGET_DIR)/usr/share/zoneinfo \
		$(TARGET_DIR)$(MINUI_SHARE_DIR)/res \
		$(TARGET_DIR)$(MINUI_SHARE_DIR)/cores \
		$(TARGET_DIR)$(MINUI_SHARE_DIR)/defaults
	rm -rf $(TARGET_DIR)/usr/share/zoneinfo/minui
	$(MINUI_ZIC) -b fat -d $(TARGET_DIR)/usr/share/zoneinfo \
		$(MINUI_TIMEZONE_SRC)
	ln -sfn ../usr/share/zoneinfo/minui/GMT+03_00 \
		$(TARGET_DIR)/etc/localtime
	printf '%s\n' 'GMT +03:00' > $(TARGET_DIR)/etc/timezone
	cp -a $(MINUI_ASSETS_DIR)/res/. $(TARGET_DIR)$(MINUI_SHARE_DIR)/res/
	cp -a $(MINUI_ASSETS_DIR)/cores/. $(TARGET_DIR)$(MINUI_SHARE_DIR)/cores/
	$(INSTALL) -D -m 0644 $(MINUI_ASSETS_DIR)/system.cfg \
		$(TARGET_DIR)$(MINUI_SHARE_DIR)/system.cfg
	$(INSTALL) -D -m 0644 $(MINUI_ASSETS_DIR)/dat/systems.cfg \
		$(TARGET_DIR)$(MINUI_SHARE_DIR)/systems.cfg
	$(INSTALL) -D -m 0644 $(MINUI_ASSETS_DIR)/defaults/asound.conf \
		$(TARGET_DIR)$(MINUI_SHARE_DIR)/defaults/asound.conf
	if [ -f "$(MINUI_ASSETS_DIR)/import-manifest.txt" ]; then \
		$(INSTALL) -D -m 0644 $(MINUI_ASSETS_DIR)/import-manifest.txt \
			$(TARGET_DIR)$(MINUI_SHARE_DIR)/import-manifest.txt; \
	fi
endef

$(eval $(generic-package))
