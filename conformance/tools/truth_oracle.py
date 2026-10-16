#!/usr/bin/env python3
"""Independent truth oracle for the scalar and key contract (§4, §5.1, §8, §13.1).

Written from the spec text, not from the Rust resolver. Generates:
  fixtures/truth/booleans.tsv    scalar resolution by word x position x style x tag
  fixtures/truth/collisions.tsv  sibling-key duplicate detection
  fixtures/truth/numbers.tsv     integer/float resolution and typed numeric targets

Columns are tab-separated; YAML text and string values are JSON-encoded.
Expected notation (shared with fixtures/truth/golden.tsv):
  null | bool:true | int:<decimal> | float:<16 hex bits> | str:<json> |
  tagged:<json uri>:<inner> | err:<snake_case kind> | syntax
Typed columns: the same notation for successes, err:<kind> for failures, - when not probed.

Usage: python3 -I scripts/truth_oracle.py OUTDIR
"""
import itertools, json, math, re, struct, sys
from pathlib import Path

# ---- §4.1 Core schema, full-match patterns in order -------------------------
NULL = {"~", "null", "Null", "NULL", ""}
TRUE = {"true", "True", "TRUE"}
FALSE = {"false", "False", "FALSE"}
INT_DEC = re.compile(r"[-+]?[0-9]+\Z")
INT_OCT = re.compile(r"0o[0-7]+\Z")
INT_HEX = re.compile(r"0x[0-9a-fA-F]+\Z")
FLOAT = re.compile(r"[-+]?(\.[0-9]+|[0-9]+(\.[0-9]*)?)([eE][-+]?[0-9]+)?\Z")
INF = re.compile(r"[-+]?(\.inf|\.Inf|\.INF)\Z")
NAN = re.compile(r"(\.nan|\.NaN|\.NAN)\Z")
I128_MIN, U128_MAX = -(2**127), 2**128 - 1


def fbits(x):
    if math.isnan(x):
        return "7ff8000000000000"
    return struct.pack(">d", x).hex()


def parse_int(t):
    if INT_OCT.match(t):
        return int(t[2:], 8)
    if INT_HEX.match(t):
        return int(t[2:], 16)
    return int(t, 10)


def int_value(t):
    n = parse_int(t)
    if n < I128_MIN or n > U128_MAX:
        return "err:integer_out_of_range"
    return f"int:{n}"


def float_value(t):
    if INF.match(t):
        return "float:" + fbits(-math.inf if t.startswith("-") else math.inf)
    if NAN.match(t):
        return "float:" + fbits(math.nan)
    x = float(t)  # correctly rounded; underflow rounds to nearest
    if math.isinf(x):
        return "err:float_out_of_range"
    return "float:" + fbits(x)


def core_plain(t):
    if t in NULL:
        return "null"
    if t in TRUE:
        return "bool:true"
    if t in FALSE:
        return "bool:false"
    if INT_DEC.match(t) or INT_OCT.match(t) or INT_HEX.match(t):
        return int_value(t)
    if FLOAT.match(t) or INF.match(t) or NAN.match(t):
        return float_value(t)
    return "str:" + json.dumps(t, ensure_ascii=False)


def resolve(text, plain, tag):
    """§4.1, §4.3, §4.4 for one scalar. tag is None or the tag as written."""
    s = "str:" + json.dumps(text, ensure_ascii=False)
    if tag is None:
        return core_plain(text) if plain else s
    if tag == "!!str" or tag == "!":
        return s
    if tag == "!!null":
        return "null" if text in NULL else "err:tag_value_mismatch"
    if tag == "!!bool":
        if text in TRUE:
            return "bool:true"
        if text in FALSE:
            return "bool:false"
        return "err:tag_value_mismatch"
    if tag == "!!int":
        if INT_DEC.match(text) or INT_OCT.match(text) or INT_HEX.match(text):
            return int_value(text)
        return "err:tag_value_mismatch"
    if tag == "!!float":
        if INT_DEC.match(text) or INT_OCT.match(text) or INT_HEX.match(text):
            n = parse_int(text)
            if float(n) != n:
                return "err:tag_value_mismatch"
            return "float:" + fbits(float(n))
        if FLOAT.match(text) or INF.match(text) or NAN.match(text):
            return float_value(text)
        return "err:tag_value_mismatch"
    if tag.startswith("!") and not tag.startswith("!!"):
        return "tagged:" + json.dumps(tag) + ":" + s  # §4.4 raw String inner
    raise ValueError(tag)


# ---- typed targets (§8) ---------------------------------------------------
def typed_bool(v):
    if v.startswith("err:"):
        return v
    return v if v.startswith("bool:") else "err:type"


def typed_string(v):
    if v.startswith("err:"):
        return v
    return v if v.startswith("str:") else "err:type"


def typed_opt_bool(v):
    if v.startswith("err:"):
        return v
    if v == "null":
        return "none"
    return v if v.startswith("bool:") else "err:type"


# ---- generation -------------------------------------------------------------
def case_variants(w):
    out = set()
    for mask in itertools.product([0, 1], repeat=len(w)):
        out.add("".join(c.upper() if m else c.lower() for c, m in zip(w, mask)))
    return sorted(out)


WORDS = []
for w in ["true", "false", "null", "y", "yes", "n", "no", "on", "off"]:
    WORDS += case_variants(w)
WORDS += ["~", "", "1", "0"]

STYLES = ["plain", "single", "double", "literal", "literal_strip", "literal_keep",
          "folded", "folded_strip", "folded_keep"]
BLOCK = {"literal": "|", "literal_strip": "|-", "literal_keep": "|+",
         "folded": ">", "folded_strip": ">-", "folded_keep": ">+"}
TAGS = [None, "!!bool", "!!str", "!!null", "!", "!custom"]
POSITIONS = ["block_value", "block_implicit_key", "block_explicit_key", "block_seq",
             "flow_seq", "flow_key", "flow_value", "top"]
KEY_POSITIONS = {"block_implicit_key", "block_explicit_key", "flow_key"}
FLOW_POSITIONS = {"flow_seq", "flow_key", "flow_value"}


def inline(word, style, tag):
    """Scalar as written inline (non-block styles)."""
    body = {"plain": word, "single": "'" + word + "'", "double": json.dumps(word)}[style]
    return (tag + " " + body) if tag else body


def block_head(style, tag):
    return (tag + " " if tag else "") + BLOCK[style]


def document(word, style, tag, pos):
    """YAML text and validity for one row, or None when the row is not generated."""
    block = style in BLOCK
    if word == "" and (pos in KEY_POSITIONS or pos in ("flow_seq", "top")) and style == "plain":
        return None  # empty plain key/item/top is a different construct; covered in golden rows
    if block and word == "":
        return None
    if block:
        if pos in FLOW_POSITIONS or pos == "block_implicit_key":
            # Block scalars cannot appear in flow context or as implicit keys.
            head = block_head(style, tag)
            y = {"flow_seq": f"[{head}\n  {word}\n]\n", "flow_key": f"{{{head}\n  {word}\n: v}}\n",
                 "flow_value": f"{{k: {head}\n  {word}\n}}\n",
                 "block_implicit_key": f"{head}\n  {word}\n: v\n"}[pos]
            return y, False
        head = block_head(style, tag)
        y = {"block_value": f"k: {head}\n  {word}\n", "block_explicit_key": f"? {head}\n  {word}\n: v\n",
             "block_seq": f"- {head}\n  {word}\n", "top": f"{head}\n  {word}\n"}[pos]
        return y, True
    s = inline(word, style, tag)
    if style == "plain" and word == "" and tag:
        s = tag  # tag with empty plain content
    y = {"block_value": f"k: {s}\n", "block_implicit_key": f"{s}: v\n", "block_explicit_key": f"? {s}\n: v\n",
         "block_seq": f"- {s}\n", "flow_seq": f"[{s}]\n", "flow_key": f"{{{s}: v}}\n",
         "flow_value": f"{{k: {s}}}\n", "top": f"{s}\n"}[pos]
    return y, True


def block_text(word, style):
    ind = BLOCK[style]
    return word if ind.endswith("-") else word + "\n"  # single content line, then end of input


def gen_booleans():
    rows = []
    n = 0
    for word, style, tag, pos in itertools.product(WORDS, STYLES, TAGS, POSITIONS):
        d = document(word, style, tag, pos)
        if d is None:
            continue
        y, valid = d
        n += 1
        rid = f"b{n:05d}"
        if not valid:
            rows.append((rid, "syntax_invalid", pos, style, tag or "-", word, y, "syntax", "-", "-", "-"))
            continue
        text = block_text(word, style) if style in BLOCK else word
        v = resolve(text, style == "plain", tag)
        cat = "resolution_error" if v.startswith("err:") else "value"
        if pos == "block_value":
            tb, ts, to = typed_bool(v), typed_string(v), typed_opt_bool(v)
            if v.startswith("tagged:"):
                tb = ts = to = "err:type"
        else:
            tb = ts = to = "-"
        rows.append((rid, cat, pos, style, tag or "-", word, y, v, tb, ts, to))
    return rows


def gen_collisions(rows_words):
    """Two sibling plain/double-quoted keys; duplicate iff §5.1 key_eq holds."""
    keys = [(w, st) for w in rows_words if w != "" for st in ("plain", "double")]
    out = []
    n = 0
    for (w1, s1), (w2, s2) in itertools.combinations(keys, 2):
        v1, v2 = resolve(w1, s1 == "plain", None), resolve(w2, s2 == "plain", None)
        y = f"{inline(w1, s1, None)}: 1\n{inline(w2, s2, None)}: 2\n"
        n += 1
        out.append((f"c{n:05d}", w1, s1, w2, s2, y, "err:duplicate_key" if v1 == v2 else "ok"))
    return out


NUM_TEXTS = [
    "0", "1", "-1", "+1", "007", "-0", "+0", "0o17", "0o8", "0x1F", "0xff", "0XFF", "-0x1", "0b101",
    "1_000", "1.0", "1.", ".5", "-.5", "1e3", "1E3", "1e+3", "1.5e-3", "-0.0", "0.0", "1e9999", "-1e9999",
    "1e-400", "2.5e-324", "4.9e-324", "3.4028235e38", "3.5e38", "1e39", "16777217", "9007199254740993",
    ".inf", "-.inf", "+.inf", ".Inf", ".INF", ".nan", ".NaN", ".NAN", "inf", "nan", "-.nan",
    "127", "128", "-128", "-129", "255", "256", "65535", "65536", "4294967295", "4294967296",
    "18446744073709551615", "18446744073709551616", "-9223372036854775808", "-9223372036854775809",
    str(U128_MAX), str(U128_MAX + 1), str(I128_MIN), str(I128_MIN - 1),
    str(2**127 - 1), str(2**127), "1:20", "0755",
    # f32 overflow threshold is the midpoint 2^128 - 2^103 (ties to even -> inf).
    "3.4028234663852886e38", "-3.4028235e38", "3.4028235677973362e38", "3.4028235677973366e38",
    "-3.4028235677973362e38", "-3.4028235677973366e38", "3.4028236e38",
]
INT_TARGETS = {"u8": (0, 2**8 - 1), "u16": (0, 2**16 - 1), "u32": (0, 2**32 - 1), "u64": (0, 2**64 - 1),
               "u128": (0, U128_MAX), "i8": (-(2**7), 2**7 - 1), "i16": (-(2**15), 2**15 - 1),
               "i32": (-(2**31), 2**31 - 1), "i64": (-(2**63), 2**63 - 1), "i128": (I128_MIN, 2**127 - 1)}


def typed_num(v, target):
    if v.startswith("err:"):
        return v
    if target in INT_TARGETS:
        if not v.startswith("int:"):
            return "err:type"
        n = int(v[4:])
        lo, hi = INT_TARGETS[target]
        return v if lo <= n <= hi else "err:integer_out_of_range"
    if v.startswith("int:"):
        n = int(v[4:])
        if target == "f64":
            return "float:" + fbits(float(n)) if float(n) == n else "err:type"
        try:
            f = struct.unpack(">f", struct.pack(">f", float(n)))[0]
        except OverflowError:
            return "err:type"
        return ("f32:" + struct.pack(">f", f).hex()) if f == n else "err:type"
    if not v.startswith("float:"):
        return "err:type"
    x = struct.unpack(">d", bytes.fromhex(v[6:]))[0]
    if target == "f64":
        return v
    if math.isnan(x):
        return "f32:7fc00000"
    try:
        return "f32:" + struct.pack(">f", x).hex()
    except OverflowError:
        return "err:float_out_of_range"


def gen_numbers():
    out = []
    n = 0
    targets = list(INT_TARGETS) + ["f32", "f64"]
    for t in NUM_TEXTS:
        for style, tag in [("plain", None), ("double", None), ("double", "!!int"), ("plain", "!!float"),
                           ("plain", "!!str")]:
            v = resolve(t, style == "plain", tag)
            y = "k: " + inline(t, style, tag) + "\n"
            n += 1
            out.append((f"n{n:04d}", t, style, tag or "-", y, v) + tuple(typed_num(v, tg) for tg in targets))
    return out, targets


def write(path, header, rows):
    with open(path, "w", encoding="utf-8") as f:
        f.write("\t".join(header) + "\n")
        for r in rows:
            f.write("\t".join(json.dumps(c, ensure_ascii=False) if i in ENC.get(path.name, ()) else c
                              for i, c in enumerate(r)) + "\n")


ENC = {"booleans.tsv": (5, 6), "collisions.tsv": (1, 3, 5), "numbers.tsv": (1, 4)}

if __name__ == "__main__":
    out = Path(sys.argv[1])
    out.mkdir(parents=True, exist_ok=True)
    b = gen_booleans()
    write(out / "booleans.tsv", ["id", "category", "position", "style", "tag", "word", "yaml", "expected",
                                 "typed_bool", "typed_string", "typed_option_bool"], b)
    c = gen_collisions(WORDS)
    write(out / "collisions.tsv", ["id", "key1", "style1", "key2", "style2", "yaml", "expected"], c)
    nrows, targets = gen_numbers()
    write(out / "numbers.tsv", ["id", "text", "style", "tag", "yaml", "expected"] + ["typed_" + t for t in targets],
          nrows)
    cats = {}
    for r in b:
        cats[r[1]] = cats.get(r[1], 0) + 1
    print(f"booleans {len(b)} {cats}; collisions {len(c)}; numbers {len(nrows)}")
