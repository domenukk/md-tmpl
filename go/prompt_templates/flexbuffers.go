package prompt_templates

import (
	"encoding/binary"
	"fmt"
	"math"
	"reflect"
	"sort"
	"strings"
	"sync"
)

type bitWidth int

const (
	w8  bitWidth = 0
	w16 bitWidth = 1
	w32 bitWidth = 2
	w64 bitWidth = 3
)

func widthU(v uint64) bitWidth {
	if v < 1<<8 {
		return w8
	} else if v < 1<<16 {
		return w16
	} else if v < 1<<32 {
		return w32
	}
	return w64
}

func widthI(v int64) bitWidth {
	uv := uint64(v) * 2
	if v < 0 {
		uv = uint64(^v) * 2
	}
	return widthU(uv)
}

func widthF(v float64) bitWidth {
	if float64(float32(v)) == v {
		return w32
	}
	return w64
}

type flexType int

const (
	typeNull          flexType = 0
	typeInt           flexType = 1
	typeUInt          flexType = 2
	typeFloat         flexType = 3
	typeKey           flexType = 4
	typeString        flexType = 5
	typeIndirectInt   flexType = 6
	typeIndirectUInt  flexType = 7
	typeIndirectFloat flexType = 8
	typeMap           flexType = 9
	typeVector        flexType = 10
	typeBlob          flexType = 25
	typeBool          flexType = 26
)

func isInline(t flexType) bool {
	return t <= typeFloat || t == typeBool
}

func packType(t flexType, bw bitWidth) byte {
	return byte((int(t) << 2) | int(bw))
}

type flexValue struct {
	val         int64
	typ         flexType
	minBitWidth bitWidth
}

func (v flexValue) elemWidth(bufSize int, elemIndex int) bitWidth {
	if isInline(v.typ) {
		return v.minBitWidth
	}
	for _, bw := range []bitWidth{w8, w16, w32, w64} {
		byteWidth := 1 << bw
		padding := (-bufSize) & (byteWidth - 1)
		offsetLoc := bufSize + padding + elemIndex*byteWidth
		offset := offsetLoc - int(v.val)
		if offset >= 0 && widthU(uint64(offset)) <= bw {
			return bw
		}
	}
	return w64
}

func (v flexValue) storedWidth(parentBitWidth bitWidth) bitWidth {
	if isInline(v.typ) {
		if v.minBitWidth > parentBitWidth {
			return v.minBitWidth
		}
		return parentBitWidth
	}
	return v.minBitWidth
}

type flexBuilder struct {
	buf   []byte
	stack []flexValue
	pairs []flexPair
	keys  []flexValue
	vals  []flexValue
	elems []flexValue
}

var builderPool = sync.Pool{
	New: func() any {
		return &flexBuilder{
			buf:   make([]byte, 0, 4096),
			stack: make([]flexValue, 0, 128),
			pairs: make([]flexPair, 0, 64),
			keys:  make([]flexValue, 0, 64),
			vals:  make([]flexValue, 0, 64),
			elems: make([]flexValue, 0, 128),
		}
	},
}

func (b *flexBuilder) align(alignment bitWidth) int {
	byteWidth := 1 << alignment
	padding := (-len(b.buf)) & (byteWidth - 1)
	for i := 0; i < padding; i++ {
		b.buf = append(b.buf, 0)
	}
	return byteWidth
}

func (b *flexBuilder) writeInt(v int64, byteWidth int) {
	switch byteWidth {
	case 1:
		b.buf = append(b.buf, byte(v))
	case 2:
		var tmp [2]byte
		binary.LittleEndian.PutUint16(tmp[:], uint16(v))
		b.buf = append(b.buf, tmp[:]...)
	case 4:
		var tmp [4]byte
		binary.LittleEndian.PutUint32(tmp[:], uint32(v))
		b.buf = append(b.buf, tmp[:]...)
	case 8:
		var tmp [8]byte
		binary.LittleEndian.PutUint64(tmp[:], uint64(v))
		b.buf = append(b.buf, tmp[:]...)
	}
}

func (b *flexBuilder) writeUInt(v uint64, byteWidth int) {
	b.writeInt(int64(v), byteWidth)
}

func (b *flexBuilder) writeFloat(v float64, byteWidth int) {
	if byteWidth == 4 {
		b.writeUInt(uint64(math.Float32bits(float32(v))), 4)
	} else {
		b.writeUInt(math.Float64bits(v), 8)
	}
}

func (b *flexBuilder) writeOffset(offset int, byteWidth int) {
	rel := len(b.buf) - offset
	b.writeUInt(uint64(rel), byteWidth)
}

func (b *flexBuilder) writeAny(v flexValue, byteWidth int) {
	switch v.typ {
	case typeNull, typeBool, typeInt, typeUInt:
		b.writeInt(v.val, byteWidth)
	case typeFloat:
		b.writeFloat(math.Float64frombits(uint64(v.val)), byteWidth)
	default:
		b.writeOffset(int(v.val), byteWidth)
	}
}

func (b *flexBuilder) addNull() {
	b.stack = append(b.stack, flexValue{val: 0, typ: typeNull, minBitWidth: w8})
}

func (b *flexBuilder) addBool(v bool) {
	val := int64(0)
	if v {
		val = 1
	}
	b.stack = append(b.stack, flexValue{val: val, typ: typeBool, minBitWidth: w8})
}

func (b *flexBuilder) addInt(v int64) {
	b.stack = append(b.stack, flexValue{val: v, typ: typeInt, minBitWidth: widthI(v)})
}

func (b *flexBuilder) addUInt(v uint64) {
	b.stack = append(b.stack, flexValue{val: int64(v), typ: typeUInt, minBitWidth: widthU(v)})
}

func (b *flexBuilder) addFloat(v float64) {
	b.stack = append(b.stack, flexValue{val: int64(math.Float64bits(v)), typ: typeFloat, minBitWidth: widthF(v)})
}

func (b *flexBuilder) addKey(key string) {
	loc := len(b.buf)
	b.buf = append(b.buf, []byte(key)...)
	b.buf = append(b.buf, 0)
	b.stack = append(b.stack, flexValue{val: int64(loc), typ: typeKey, minBitWidth: w8})
}

func (b *flexBuilder) addString(s string) {
	data := []byte(s)
	bw := widthU(uint64(len(data)))
	byteWidth := b.align(bw)
	b.writeUInt(uint64(len(data)), byteWidth)
	loc := len(b.buf)
	b.buf = append(b.buf, data...)
	b.buf = append(b.buf, 0)
	b.stack = append(b.stack, flexValue{val: int64(loc), typ: typeString, minBitWidth: bw})
}

func (b *flexBuilder) createVector(elements []flexValue, typed bool, keys *flexValue) flexValue {
	length := len(elements)
	bw := widthU(uint64(length))
	prefixElems := 1
	if keys != nil {
		if keys.elemWidth(len(b.buf), 0) > bw {
			bw = keys.elemWidth(len(b.buf), 0)
		}
		prefixElems += 2
	}
	for i, e := range elements {
		ew := e.elemWidth(len(b.buf), prefixElems+i)
		if ew > bw {
			bw = ew
		}
	}
	byteWidth := b.align(bw)
	if keys != nil {
		b.writeOffset(int(keys.val), byteWidth)
		b.writeUInt(uint64(1<<keys.minBitWidth), byteWidth)
	}
	b.writeUInt(uint64(length), byteWidth)
	loc := len(b.buf)
	for _, e := range elements {
		b.writeAny(e, byteWidth)
	}
	if !typed {
		for _, e := range elements {
			b.buf = append(b.buf, packType(e.typ, e.storedWidth(bw)))
		}
	}
	typ := typeVector
	if keys != nil {
		typ = typeMap
	}
	return flexValue{val: int64(loc), typ: typ, minBitWidth: bw}
}

type flexPair struct {
	key flexValue
	val flexValue
}

func (b *flexBuilder) readKey(offset int) string {
	end := offset
	for end < len(b.buf) && b.buf[end] != 0 {
		end++
	}
	return string(b.buf[offset:end])
}

func (b *flexBuilder) endMap(start int) {
	b.pairs = b.pairs[:0]
	for i := start; i < len(b.stack); i += 2 {
		b.pairs = append(b.pairs, flexPair{key: b.stack[i], val: b.stack[i+1]})
	}
	sort.Slice(b.pairs, func(i, j int) bool {
		return b.readKey(int(b.pairs[i].key.val)) < b.readKey(int(b.pairs[j].key.val))
	})
	b.keys = b.keys[:0]
	b.vals = b.vals[:0]
	for _, p := range b.pairs {
		b.keys = append(b.keys, p.key)
		b.vals = append(b.vals, p.val)
	}
	b.stack = b.stack[:start]
	keysVec := b.createVector(b.keys, true, nil)
	mapVal := b.createVector(b.vals, false, &keysVec)
	b.stack = append(b.stack, mapVal)
}

func (b *flexBuilder) endVector(start int) {
	b.elems = b.elems[:0]
	b.elems = append(b.elems, b.stack[start:]...)
	b.stack = b.stack[:start]
	vecVal := b.createVector(b.elems, false, nil)
	b.stack = append(b.stack, vecVal)
}

func (b *flexBuilder) finish() ([]byte, error) {
	if len(b.stack) != 1 {
		return nil, fmt.Errorf("internal stack size must be 1, got %d", len(b.stack))
	}
	root := b.stack[0]
	byteWidth := b.align(root.elemWidth(len(b.buf), 0))
	b.writeAny(root, byteWidth)
	b.buf = append(b.buf, packType(root.typ, root.storedWidth(w8)))
	b.buf = append(b.buf, byte(byteWidth))
	return b.buf, nil
}

func marshalFlexbuffers(v any) ([]byte, error) {
	b := builderPool.Get().(*flexBuilder)
	b.buf = b.buf[:0]
	b.stack = b.stack[:0]
	b.pairs = b.pairs[:0]
	b.keys = b.keys[:0]
	b.vals = b.vals[:0]
	b.elems = b.elems[:0]
	defer builderPool.Put(b)

	if err := b.marshalValue(reflect.ValueOf(v)); err != nil {
		return nil, err
	}
	finished, err := b.finish()
	if err != nil {
		return nil, err
	}
	res := make([]byte, len(finished))
	copy(res, finished)
	return res, nil
}

func (b *flexBuilder) marshalValue(val reflect.Value) error {
	if !val.IsValid() {
		b.addNull()
		return nil
	}
	// Check for Variant custom marshaling via type assertion.
	if variant, ok := val.Interface().(Variant); ok {
		if len(variant.Fields) == 0 {
			b.addString(variant.Kind)
			return nil
		}
		start := len(b.stack)
		b.addKey("__kind__")
		b.addString(variant.Kind)
		for k, fv := range variant.Fields {
			b.addKey(k)
			if err := b.marshalValue(reflect.ValueOf(fv)); err != nil {
				return err
			}
		}
		b.endMap(start)
		return nil
	}

	switch val.Kind() {
	case reflect.Ptr, reflect.Interface:
		if val.IsNil() {
			b.addNull()
			return nil
		}
		return b.marshalValue(val.Elem())
	case reflect.Bool:
		b.addBool(val.Bool())
	case reflect.Int, reflect.Int8, reflect.Int16, reflect.Int32, reflect.Int64:
		b.addInt(val.Int())
	case reflect.Uint, reflect.Uint8, reflect.Uint16, reflect.Uint32, reflect.Uint64:
		b.addUInt(val.Uint())
	case reflect.Float32, reflect.Float64:
		b.addFloat(val.Float())
	case reflect.String:
		b.addString(val.String())
	case reflect.Slice, reflect.Array:
		start := len(b.stack)
		for i := 0; i < val.Len(); i++ {
			if err := b.marshalValue(val.Index(i)); err != nil {
				return err
			}
		}
		b.endVector(start)
	case reflect.Map:
		start := len(b.stack)
		iter := val.MapRange()
		for iter.Next() {
			k := iter.Key()
			if k.Kind() != reflect.String {
				return fmt.Errorf("map key must be string, got %s", k.Kind())
			}
			b.addKey(k.String())
			if err := b.marshalValue(iter.Value()); err != nil {
				return err
			}
		}
		b.endMap(start)
	case reflect.Struct:
		start := len(b.stack)
		if err := b.marshalStructFields(val); err != nil {
			return err
		}
		b.endMap(start)
	default:
		return fmt.Errorf("unsupported type: %s", val.Kind())
	}
	return nil
}

// marshalStructFields serializes struct fields into the current FlexBuffer map,
// promoting anonymous (embedded) struct fields to the parent level, matching
// encoding/json semantics.
func (b *flexBuilder) marshalStructFields(val reflect.Value) error {
	typ := val.Type()
	for i := 0; i < typ.NumField(); i++ {
		field := typ.Field(i)
		if !field.IsExported() {
			continue
		}

		tag := field.Tag.Get("json")

		// Anonymous (embedded) struct without an explicit json name:
		// promote its sub-fields to the parent map level.
		if field.Anonymous && field.Type.Kind() == reflect.Struct && (tag == "" || strings.HasPrefix(tag, ",")) {
			if err := b.marshalStructFields(val.Field(i)); err != nil {
				return err
			}
			continue
		}

		name := field.Name
		omitempty := false
		if tag != "" {
			parts := strings.Split(tag, ",")
			if parts[0] == "-" {
				continue
			}
			if parts[0] != "" {
				name = parts[0]
			}
			for _, opt := range parts[1:] {
				if opt == "omitempty" {
					omitempty = true
				}
			}
		}
		fieldVal := val.Field(i)
		if omitempty && isZeroValue(fieldVal) {
			continue
		}
		b.addKey(name)
		if err := b.marshalValue(fieldVal); err != nil {
			return err
		}
	}
	return nil
}

// isZeroValue reports whether v is the zero value for its type, matching
// encoding/json omitempty semantics.
func isZeroValue(v reflect.Value) bool {
	switch v.Kind() {
	case reflect.Bool:
		return !v.Bool()
	case reflect.Int, reflect.Int8, reflect.Int16, reflect.Int32, reflect.Int64:
		return v.Int() == 0
	case reflect.Uint, reflect.Uint8, reflect.Uint16, reflect.Uint32, reflect.Uint64:
		return v.Uint() == 0
	case reflect.Float32, reflect.Float64:
		return v.Float() == 0
	case reflect.String:
		return v.Len() == 0
	case reflect.Slice, reflect.Map:
		return v.IsNil() || v.Len() == 0
	case reflect.Ptr, reflect.Interface:
		return v.IsNil()
	case reflect.Array:
		return v.Len() == 0
	case reflect.Struct:
		return false // structs are never "empty" for omitempty
	default:
		return false
	}
}
