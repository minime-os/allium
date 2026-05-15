################################################################################
#
# allium
#
################################################################################

ALLIUM_SRC_DIR = $(realpath $(BR2_EXTERNAL_ALLIUM_EXTERNAL_PATH)/..)
ALLIUM_PAYLOAD_DIR = $(call qstrip,$(BR2_PACKAGE_ALLIUM_PAYLOAD_DIR))

define ALLIUM_INSTALL_TARGET_CMDS
	test -d "$(ALLIUM_PAYLOAD_DIR)"
	rm -rf $(TARGET_DIR)/usr/share/allium-sdcard
	mkdir -p $(TARGET_DIR)/usr/share/allium-sdcard
	cp -a "$(ALLIUM_PAYLOAD_DIR)/." $(TARGET_DIR)/usr/share/allium-sdcard/
endef

$(eval $(generic-package))
