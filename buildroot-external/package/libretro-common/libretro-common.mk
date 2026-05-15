################################################################################
# libretro-common
################################################################################

LIBRETRO_COMMON_VERSION = 668749ae38a9e85744d1c15a652a1e8db8ab9e82
LIBRETRO_COMMON_SITE = $(call github,libretro,libretro-common,$(LIBRETRO_COMMON_VERSION))
LIBRETRO_COMMON_LICENSE = MIT
LIBRETRO_COMMON_LICENSE_FILES = include/libretro.h
LIBRETRO_COMMON_INSTALL_STAGING = YES

define LIBRETRO_COMMON_INSTALL_STAGING_CMDS
	$(INSTALL) -D -m 0644 $(@D)/include/libretro.h \
		$(STAGING_DIR)/usr/include/libretro.h
endef

$(eval $(generic-package))
