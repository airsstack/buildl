package main

import (
	"fmt"

	"example.com/polyglot/schema"
)

func main() {
	fmt.Printf("go service %s: %s\n", schema.Version, schema.Greeting)
}
