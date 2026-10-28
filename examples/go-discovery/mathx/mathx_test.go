package mathx

import "testing"

func TestSum(t *testing.T) {
	if got := Sum(1, 2, 3); got != 6 {
		t.Fatalf("Sum(1, 2, 3) = %d", got)
	}
}
