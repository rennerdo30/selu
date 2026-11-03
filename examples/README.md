# Examples

Small addons written in Selu. Each one builds with `selu build`:

```
cargo run -p selu-cli -- build examples/<name>
```

and writes an addon folder to `examples/<name>/dist/<Name>/` that you can
copy into `World of Warcraft/_<flavour>_/Interface/AddOns`.

| Example | Shows |
|---|---|
| [`modules`](modules/src/main.selu) | named imports, live exports, shared addon namespace and dependency ordering |
| [`hello`](hello/src/main.selu) | the smallest addon: one event handler |
| [`loot-summary`](loot-summary/src/main.selu) | arrays of objects, `Map`, `for...of`, template strings, `??`, `filter`/`map` with arrow functions, `Math` |
| [`cooldowns`](cooldowns/src/main.selu) | functions returning several values and destructuring, `? :`, `continue`, `+=`/`++`, closures with their own state, `sort` with a compare function |
| [`slash-and-hooks`](slash-and-hooks/src/main.selu) | slash command registration, shared status callback, additive hooks, logging and localised labels |

Every example with an `expected.txt` is also a test: `cargo test` builds it
with the real `selu` binary, loads it into the simulator in `.toc` order,
fires `PLAYER_LOGIN` and compares what it printed with `expected.txt`
(`crates/selu-cli/tests/examples.rs`). A new example needs that file, so it
can't silently break.
