package util

import (
	"cmp"
	"fmt"
	"slices"
)

// Matches OrderBy(o => o.Name, StringComparer.Ordinal), with stable sort, ascending by default
func OrderedByNameAscii[T fmt.Stringer](in []T) []T {
	out := slices.Clone(in)
	slices.SortStableFunc(out, func(a, b T) int {
		return cmp.Compare(a.String(), b.String())
	})
	return out
}
