# Xtrieve C SDK Makefile

CC ?= gcc
CFLAGS = -Wall -Wextra -O2 -fPIC
LDFLAGS =

# Platform detection
UNAME_S := $(shell uname -s)
ifeq ($(UNAME_S),Linux)
    LDFLAGS += -lpthread
endif
ifeq ($(UNAME_S),Darwin)
    # macOS
endif

# Targets
STATIC_LIB = libxtrieve.a
SHARED_LIB = libxtrieve.so
ifeq ($(UNAME_S),Darwin)
    SHARED_LIB = libxtrieve.dylib
endif

OBJS = xtrieve.o
EXAMPLE = example

.PHONY: all clean static shared example install

all: static shared

static: $(STATIC_LIB)

shared: $(SHARED_LIB)

$(STATIC_LIB): $(OBJS)
	ar rcs $@ $^

$(SHARED_LIB): $(OBJS)
ifeq ($(UNAME_S),Darwin)
	$(CC) -shared -o $@ $^ $(LDFLAGS)
else
	$(CC) -shared -o $@ $^ $(LDFLAGS)
endif

%.o: %.c xtrieve.h
	$(CC) $(CFLAGS) -c $< -o $@

example: example.c $(STATIC_LIB)
	$(CC) $(CFLAGS) -o $(EXAMPLE) example.c -L. -lxtrieve $(LDFLAGS)

clean:
	rm -f $(OBJS) $(STATIC_LIB) $(SHARED_LIB) $(EXAMPLE)

install: static shared
	install -d $(DESTDIR)/usr/local/lib
	install -d $(DESTDIR)/usr/local/include
	install -m 644 $(STATIC_LIB) $(DESTDIR)/usr/local/lib/
	install -m 755 $(SHARED_LIB) $(DESTDIR)/usr/local/lib/
	install -m 644 xtrieve.h $(DESTDIR)/usr/local/include/
