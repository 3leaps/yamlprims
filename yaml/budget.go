package yaml

import "math/bits"

const (
	// NodeOverhead is the fixed logical byte weight charged per Node (spec-v1.4 §6).
	NodeOverhead uint64 = 64
	// EntryOverhead is the fixed logical byte weight charged per Mapping entry (spec-v1.4 §6).
	EntryOverhead uint64 = 24
)

type budget struct {
	limits      Limits
	nodes       uint64
	expanded    uint64
	owned       uint64
	aliasEvents uint64
}

func newBudget(limits Limits) budget {
	return budget{limits: limits}
}

func (b *budget) charge(count, weight uint64, expanded bool) (LimitKind, int, bool) {
	nextNodes, carry := bits.Add64(b.nodes, count, 0)
	if carry != 0 || nextNodes > uint64(b.limits.MaxNodes) {
		return LimitMaxNodes, b.limits.MaxNodes, false
	}
	if expanded {
		nextExpanded, expCarry := bits.Add64(b.expanded, count, 0)
		if expCarry != 0 || nextExpanded > uint64(b.limits.MaxExpandedNodes) {
			return LimitMaxExpandedNodes, b.limits.MaxExpandedNodes, false
		}
		b.expanded = nextExpanded
	}
	nextOwned, ownCarry := bits.Add64(b.owned, weight, 0)
	if ownCarry != 0 || nextOwned > uint64(b.limits.MaxOwnedBytes) {
		return LimitMaxOwnedBytes, b.limits.MaxOwnedBytes, false
	}
	b.nodes = nextNodes
	b.owned = nextOwned
	return "", 0, true
}

func (b *budget) alias() (LimitKind, int, bool) {
	next, carry := bits.Add64(b.aliasEvents, 1, 0)
	if carry != 0 || next > uint64(b.limits.MaxAliasEvents) {
		return LimitMaxAliasEvents, b.limits.MaxAliasEvents, false
	}
	b.aliasEvents = next
	return "", 0, true
}

func tagBytes(t Tag) uint64 {
	return satAdd64(satAdd64(uint64(len(t.uri)), uint64(len(t.handle))), uint64(len(t.suffix)))
}

func satAdd64(a, b uint64) uint64 {
	sum, carry := bits.Add64(a, b, 0)
	if carry != 0 {
		return ^uint64(0)
	}
	return sum
}

func satMul64(a, b uint64) uint64 {
	hi, lo := bits.Mul64(a, b)
	if hi != 0 {
		return ^uint64(0)
	}
	return lo
}

// metrics computes (nodeCount, logicalWeight, collectionHeight) for root iteratively per spec-v1.4 §6.
func metrics(root Node) (uint64, uint64, int) {
	type step struct {
		node         *Node
		collection   bool
		childHeights int
	}
	var count uint64
	var weight uint64
	heights := make([]int, 0, 16)
	rootCopy := root
	stack := []step{{node: &rootCopy}}

	for len(stack) > 0 {
		s := stack[len(stack)-1]
		stack = stack[:len(stack)-1]
		if s.collection {
			start := len(heights) - s.childHeights
			maxChild := 0
			for _, h := range heights[start:] {
				if h > maxChild {
					maxChild = h
				}
			}
			heights = heights[:start]
			heights = append(heights, 1+maxChild)
			continue
		}
		n := s.node
		count = satAdd64(count, 1)
		weight = satAdd64(weight, NodeOverhead)
		switch n.value.kind {
		case ValueKindNull, ValueKindBool, ValueKindInt, ValueKindFloat:
			heights = append(heights, 0)
		case ValueKindString:
			weight = satAdd64(weight, uint64(len(n.value.s)))
			heights = append(heights, 0)
		case ValueKindTagged:
			if n.value.tagged != nil {
				t := n.value.tagged
				weight = satAdd64(weight, tagBytes(t.tag))
				stack = append(stack, step{node: &t.value})
			} else {
				heights = append(heights, 0)
			}
		case ValueKindSequence:
			items := n.value.seq
			stack = append(stack, step{collection: true, childHeights: len(items)})
			for i := len(items) - 1; i >= 0; i-- {
				stack = append(stack, step{node: &items[i]})
			}
		case ValueKindMapping:
			var entries []MappingEntry
			if n.value.m != nil {
				entries = n.value.m.entries
			}
			weight = satAdd64(weight, satMul64(uint64(len(entries)), EntryOverhead))
			stack = append(stack, step{collection: true, childHeights: len(entries) * 2})
			for i := len(entries) - 1; i >= 0; i-- {
				stack = append(stack, step{node: &entries[i].Value})
				stack = append(stack, step{node: &entries[i].Key})
			}
		}
	}
	h := 0
	if len(heights) > 0 {
		h = heights[len(heights)-1]
	}
	return count, weight, h
}
