package words

import "testing"

func TestCount(t *testing.T) {
	if got := Count("one two  three"); got != 3 {
		t.Fatalf("Count = %d", got)
	}
}
