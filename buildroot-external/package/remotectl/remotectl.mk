################################################################################
# remotectl
################################################################################

REMOTECTL_VERSION = local
REMOTECTL_SITE = $(BR2_EXTERNAL_ALLIUM_EXTERNAL_PATH)/package/remotectl
REMOTECTL_SITE_METHOD = local
REMOTECTL_LICENSE = See source tree

REMOTECTL_SRC_DIR = $(@D)/src
REMOTECTL_DEPENDENCIES = zlib ffmpeg x264

# Tiny standalone tools: no extra library dependencies.
define REMOTECTL_BUILD_CMDS
	mkdir -p $(@D)/build-remotectl
	$(TARGET_CC) $(TARGET_CFLAGS) -std=gnu99 -D_GNU_SOURCE \
		$(REMOTECTL_SRC_DIR)/remote-inputd.c \
		-o $(@D)/build-remotectl/remote-inputd \
		$(TARGET_LDFLAGS)
	$(TARGET_CC) $(TARGET_CFLAGS) -std=gnu99 -D_GNU_SOURCE \
		$(REMOTECTL_SRC_DIR)/remotectl.c \
		-o $(@D)/build-remotectl/remotectl \
		$(TARGET_LDFLAGS)
	$(TARGET_CC) $(TARGET_CFLAGS) -std=gnu99 -D_GNU_SOURCE \
		$(REMOTECTL_SRC_DIR)/remote-fbgrab.c \
		-o $(@D)/build-remotectl/remote-fbgrab \
		$(TARGET_LDFLAGS)
	$(TARGET_CC) $(TARGET_CFLAGS) -std=gnu99 -D_GNU_SOURCE \
		$(REMOTECTL_SRC_DIR)/remote-ppm2png.c \
		-o $(@D)/build-remotectl/remote-ppm2png \
		$(TARGET_LDFLAGS) -lz
endef

define REMOTECTL_INSTALL_TARGET_CMDS
	$(INSTALL) -D -m 0755 $(@D)/build-remotectl/remote-inputd \
		$(TARGET_DIR)/usr/bin/remote-inputd
	$(INSTALL) -D -m 0755 $(@D)/build-remotectl/remotectl \
		$(TARGET_DIR)/usr/bin/remotectl
	$(INSTALL) -D -m 0755 $(@D)/build-remotectl/remote-fbgrab \
		$(TARGET_DIR)/usr/bin/remote-fbgrab
	$(INSTALL) -D -m 0755 $(@D)/build-remotectl/remote-ppm2png \
		$(TARGET_DIR)/usr/bin/remote-ppm2png
	$(INSTALL) -D -m 0755 $(@D)/overlay/etc/init.d/S15remotectl \
		$(TARGET_DIR)/etc/init.d/S15remotectl
	$(INSTALL) -D -m 0644 $(@D)/overlay/etc/default/remotectl \
		$(TARGET_DIR)/etc/default/remotectl
	$(INSTALL) -D -m 0755 $(@D)/overlay/usr/bin/remote-screenrecctl \
		$(TARGET_DIR)/usr/bin/remote-screenrecctl
	$(INSTALL) -D -m 0755 $(@D)/overlay/usr/bin/remote-selfcheck \
		$(TARGET_DIR)/usr/bin/remote-selfcheck
endef

$(eval $(generic-package))
