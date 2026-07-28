include env.mk

PROFILE ?= release
CSR     := target/$(RUST_TARGET)/$(PROFILE)/csr
CSL     := target/$(RUST_TARGET)/$(PROFILE)/csl
DESTDIR  ?=

.PHONY: all build install clean uninstall

all: build

build:
	CARGO_TARGET_DIR=$(CURDIR)/target cargo build --target $(RUST_TARGET) --profile $(PROFILE) --locked

install: build
	install -Dm755 $(CSR) $(DESTDIR)$(PREFIX)/bin/csr
	install -Dm755 $(CSL) $(DESTDIR)$(PREFIX)/bin/csl

uninstall:
	rm -f $(DESTDIR)$(PREFIX)/bin/csr
	rm -f $(DESTDIR)$(PREFIX)/bin/csl

clean:
	cargo clean
