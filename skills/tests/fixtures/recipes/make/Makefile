CC := cc
CXX ::= c++
X ?= y
FLAGS = -O2
FLAGS += -Wall

.PHONY: all test

all: app

app: main.o util.o
	$(CC) -o $@ $^

%.o: %.c
	$(CC) -c $< -o $@

test: app
	./app --self-test
