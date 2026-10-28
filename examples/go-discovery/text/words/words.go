// Package words splits text into words.
package words

import "strings"

// Count is the number of words in text.
func Count(text string) int {
	return len(strings.Fields(text))
}
