PREFIX=/usr
UDEVDIR=$(shell pkg-config --variable=udevdir udev)
SBINDIR=$(PREFIX)/sbin
CONFDIR=/etc/mdevctl.d
MANDIR=$(PREFIX)/share/man
NAME=mdevctl
LASTVERSION:=$(shell git tag --sort=version:refname --merged| tail -n 1 | sed 's/\([0-9]*\.\)\([0-9]*\)/\2/g')
VERSION:=0.$(shell echo $$(($(LASTVERSION)+1)))
NVFMT=$(NAME)-$(VERSION)

files: mdevctl 60-mdevctl.rules mdevctl.8 \
	Makefile COPYING README.md mdevctl.spec.in

archive: files mdevctl.spec
	git archive --prefix=$(NVFMT)/ HEAD > $(NVFMT).tar
	gzip -f -9 $(NVFMT).tar

mdevctl.spec: tag mdevctl.spec.in files
	sed -e 's:#VERSION#:$(VERSION):g' < mdevctl.spec.in > mdevctl.spec
	PREV=""; \
	for TAG in `git tag --sort=version:refname | tac`; do \
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

clean:
	rm -f mdevctl.spec *.src.rpm noarch/*.rpm *.tar.gz

tag:
	@if git describe --exact-match 2>/dev/null 1>&2; then \
		echo "Current commit is already tagged as:"; \
		git describe --exact-match; \
		false; \
	else \
		if ! git tag -l $(VERSION) | grep $(VERSION); then \
			sed -i -e 's/\(^\s*echo "\$$0 version \)[0-9]*\.[0-9]*/\1$(VERSION)/g' mdevctl; \
			git commit -s -m "update version" mdevctl; \
			git tag $(VERSION) -a -m "$(VERSION)"; \
		fi; \
	fi
