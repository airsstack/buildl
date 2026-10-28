package main

import "testing"

func TestGreeting(t *testing.T) {
	if got := Greeting(); got != "hello from go" {
		t.Fatalf("Greeting() = %q", got)
	}
}
