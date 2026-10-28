// Package mathx holds small integer helpers.
package mathx

// Sum adds every value.
func Sum(values ...int) int {
	total := 0
	for _, v := range values {
		total += v
	}
	return total
}
