package util

import (
	"cmp"
	"slices"
)

type hasName interface {
	GetName() string
}

func OrderedByNameAscii[T hasName](in []T) []T {
	out := slices.Clone(in)
	slices.SortStableFunc(out, func(a, b T) int {
		return cmp.Compare(a.GetName(), b.GetName())
	})
	return out
}
