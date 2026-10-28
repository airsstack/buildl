package main

import "fmt"

// Greeting is the line the program prints.
func Greeting() string {
	return "hello from go"
}

func main() {
	fmt.Println(Greeting())
}
