# Lua string patterns

Plain strings are literal. Pattern methods require a static tagged template:

```selu
const [id, name] = link.match(luaPattern`%|Hitem:(%d+).-%|h%[(.-)%]%|h`);
for (const word of chat.matchAll(luaPattern`%S+`)) { print(word); }
```

`luaPattern` is a compiler intrinsic. Its raw template text is checked at compile
time and emitted as a Lua string; it has no runtime object. Interpolation is
forbidden. Pass the template directly to the method. Template delimiter escapes (\` and \${) produce literal delimiter characters; other backslashes are raw, so
`\d` is diagnosed at the backslash with the suggestion `%d`. Other tags (L and logging) and untagged strings/templates retain their
existing escape rules and diagnostics. A local binding named luaPattern shadows the intrinsic.

The receiver must be typed `string` (an alias of `string` counts). Calling a
pattern method with a `luaPattern` on an untyped value is an error (SELU0566),
because Lua would call the value's own method, or Lua's 1-based `match`: annotate
the value with `: string` or convert it with a template string such as `${value}`.
Objects and classes that declare their own `match`, `search`, `matchAll`,
`replace` or `replaceAll` keep calling those methods.

| Method | Result |
|---|---|
| `match(pattern)` | `string?` without captures, otherwise capture results |
| `search(pattern)` | 0-based byte start, or `null` |
| `matchAll(pattern)` | lazy iterator; captures are non-null on each turn |
| `replace(pattern, replacement)` | multiple results `(string, number)`, one substitution |
| `replaceAll(pattern, replacement)` | multiple results `(string, number)`, all substitutions |

Capture results are strings, except `()` position captures, which are 0-based
byte positions. On failure every capture result is `null`. Single-value uses
select the first result, including arguments, array elements and conditions.
Destructuring takes all results. Tuple-returning functions can forward them.
Positions and search use arithmetic temporaries, without tables or closures.
Destructuring declares your names directly (`local s, p = string.match(...)`,
then `p = p and p - 1`); only skipped slots get a temporary.
`matchAll` lowers to `for a, b in string.gmatch(...) do`; positions are adjusted
inside the loop. Stored iterators are stateful and consumed once. Iteration over
empty matches follows Lua 5.1 gmatch, including the position after the last byte.
A leading `^` is rejected for matchAll; use match for anchored input.

Replacement strings are literal: `%1` inserts those two characters rather than
expanding a capture. Percent signs are escaped by the compiler (dynamic string
replacements are escaped once). Callbacks receive typed captures, or the whole
match when there are none. Return a string to replace the match, or `null` to
keep it. Lua gsub also treats false as keeping the match, but Selu's callback
type permits only string or null. The substitution count includes kept matches.
Position parameters, including rest arguments, are converted before calling the callback.
Named callbacks and callbacks with rest parameters and positions need an adapter
closure per replacement operation, never per match. Untyped replacement values
are checked once at runtime: strings have percent signs escaped, functions get
the position adapter when needed, and other values pass through to Lua gsub.
An untyped `nil` or table replacement therefore follows Lua gsub's behaviour:
`nil` raises an error, and a table is used as a lookup by the first capture (or
the whole match).
Statically typed strings and fixed-parameter inline callbacks keep direct paths.

Lua 5.1 supports `.`, sets (`[abc]`, `[^abc]`), classes `%a %c %d %l %p %s %u
%w %x %z` and their uppercase complements, `* + - ?`, anchors `^ $`, captures,
closed string-capture references `%1` through `%9`, and balanced pairs `%b()`.
At most 32 captures are permitted. Sets, captures, references and escapes are
validated. `%g` is unavailable in Lua 5.1; use `[!-~]`. `%f[set]` matches a frontier from a byte outside the set to a byte inside it;
input boundaries act as zero bytes. It is implemented in Lua 5.1 but undocumented
in its manual: see the `case 'f'` branch in [Lua 5.1 lstrlib.c](https://www.lua.org/source/5.1/lstrlib.c.html#match).
All supported WoW clients use this Lua 5.1 implementation (parent decision), and
the mlua Lua 5.1 simulator probe and generated-method tests agree. `%g` remains
rejected because it is a Lua 5.2+ class. No client was launched.

Regex alternation `|`, counted repeats `{n}`, `\d` and non-capturing groups
`(?:...)` are errors with character spans and suggested Lua syntax. Escape
literal punctuation using `%`, for example `%|`, `%{`, `%(` and `%%`.
A backslash is an ordinary character in Lua patterns; Selu rejects every
backslash because it reads like a regex escape, and `\\` gets the suggestion to
write `%\` for a literal backslash.

WoW text uses `|cAARRGGBB` for colour, `|r` to reset colour, `|H...|hlabel|h`
for hyperlinks, and `||` for a visible pipe. Patterns see the underlying bytes,
including these control sequences: write `%|c`, `%|r`, `%|H`, `%|h`, or `%|%|`.
The item-links example captures an ID and label without depending on the item
cache, then counts words in a chat line.

Patterns can backtrack badly, especially repeated unanchored wildcard searches.
Do not match unbounded user input in hot event handlers. Prefer bounded inputs,
anchors where appropriate, and narrow classes over repeated `.-` or `.*`.
There is no regex engine, pattern object allocation, or background work.
