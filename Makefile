PREFIX=/usr
UDEVDIR=$(shell pkg-config --variable=udevdir udev)
SBINDIR=$(PREFIX)/sbin
CONFDIR=/etc/mdevctl.d
CALLOUT_CMD_DIR=$(CONFDIR)/callouts/scripts.d
CALLOUT_NOTIFIER_DIR=$(CONFDIR)/notification/notifiers.d
MANDIR=$(PREFIX)/share/man
NAME=mdevctl
MDEVCTL_VER=$(shell ./mdevctl version)
REVLIST_VER=0.$(shell git rev-list --count HEAD)
NEXT_VER=0.$(shell echo $$(( $(shell git rev-list --count HEAD) + 1 )) )
NVFMT=$(NAME)-$(REVLIST_VER)

files: mdevctl 60-mdevctl.rules mdevctl.8 \
	Makefile COPYING README.md mdevctl.spec.in

archive: files mdevctl.spec
	git archive --prefix=$(NVFMT)/ HEAD > $(NVFMT).tar
	gzip -f -9 $(NVFMT).tar

mdevctl.spec: tag mdevctl.spec.in files
	sed -e 's:#VERSION#:$(shell ./mdevctl version):g' < mdevctl.spec.in > mdevctl.spec
	PREV=""; \
	for TAG in `git tag --sort=version:refname --merged | tac`; do \
	    if [ -n "$$PREV" ]; then \
	    git log --format="- %h (\"%s\")" $$TAG..$$PREV >> mdevctl.spec; \
	    fi; \
	    git log -1 --format="%n* %cd %aN <%ae> - $$TAG-1" --date="format:%a %b %d %Y" $$TAG >> mdevctl.spec; \
	    PREV=$$TAG; \
	done; \
	git log --format="- %h (\"%s\")" $$TAG >> mdevctl.spec

srpm: mdevctl.spec archive
	rpmbuild -bs --define "_sourcedir $(PWD)" --define "_specdir $(PWD)" --define "_builddir $(PWD)" --define "_srcrpmdir $(PWD)" --define "_rpmdir $(PWD)" mdevctl.spec

rpm: mdevctl.spec archive
	rpmbuild -bb --define "_sourcedir $(PWD)" --define "_specdir $(PWD)" --define "_builddir $(PWD)" --define "_srcrpmdir $(PWD)" --define "_rpmdir $(PWD)" mdevctl.spec

install:
	mkdir -p $(DESTDIR)$(CONFDIR)
	mkdir -p $(DESTDIR)$(UDEVDIR)/rules.d/
	install -m 644 60-mdevctl.rules $(DESTDIR)$(UDEVDIR)/rules.d/
	mkdir -p $(DESTDIR)$(SBINDIR)
	install -m 755 mdevctl $(DESTDIR)$(SBINDIR)/
	ln -sf mdevctl $(DESTDIR)$(SBINDIR)/lsmdev
	mkdir -p $(DESTDIR)$(MANDIR)/man8
	install -m 644 mdevctl.8 $(DESTDIR)$(MANDIR)/man8/
	ln -sf mdevctl.8  $(DESTDIR)$(MANDIR)/man8/lsmdev.8
	mkdir -p $(DESTDIR)$(CALLOUT_CMD_DIR)
	mkdir -p $(DESTDIR)$(CALLOUT_NOTIFIER_DIR)

clean:
	rm -f mdevctl.spec *.src.rpm noarch/*.rpm *.tar.gz

tag:
	[ $(MDEVCTL_VER) == $(REVLIST_VER) ] || (sed -i "s/^version=.*/version=\"$(NEXT_VER)\"/" mdevctl && git add mdevctl && git commit -m "Automatic version commit for tag $(NEXT_VER)" && git tag $(NEXT_VER))
