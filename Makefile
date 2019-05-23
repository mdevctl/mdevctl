PREFIX=/usr
UDEVDIR=$(shell pkg-config --variable=udevdir udev)
UNITDIR=$(shell pkg-config --variable=systemdsystemunitdir systemd)
SBINDIR=$(PREFIX)/sbin
LIBEXECDIR=$(PREFIX)/libexec
CONFDIR=/etc/mdev.d
NAME=mdevctl
VERSION=0.$(shell git rev-list --count HEAD)
COMMIT=$(shell git rev-list --max-count 1 HEAD)
NVFMT=$(NAME)-$(VERSION)-$(COMMIT)

files: mdevctl.sbin mdevctl.libexec mdev@.service 60-persistent-mdev.rules \
	Makefile COPYING README

archive: files tag
	git archive --prefix=$(NVFMT)/ HEAD > $(NVFMT).tar
	gzip -f -9 $(NVFMT).tar

install:
	mkdir -p $(DESTDIR)/$(CONFDIR)
	mkdir -p $(DESTDIR)/$(UDEVDIR)/rules.d/
	install -m 644 60-persistent-mdev.rules $(DESTDIR)/$(UDEVDIR)/rules.d/
	mkdir -p $(DESTDIR)/$(UNITDIR)
	install -m 644 mdev@.service $(DESTDIR)/$(UNITDIR)/
	mkdir -p $(DESTDIR)/$(SBINDIR)
	install -m 755 mdevctl.sbin $(DESTDIR)/$(SBINDIR)/mdevctl
	mkdir -p $(DESTDIR)/$(LIBEXECDIR)
	install -m 755 mdevctl.libexec $(DESTDIR)/$(LIBEXECDIR)/mdevctl
	systemctl daemon-reload
	udevadm control --reload-rules

clean:
	rm -f mdevctl.spec *.src.rpm noarch/*.rpm *.tar.gz

tag:
	git tag -l $(VERSION) | grep -q $(VERSION) || git tag $(VERSION)
