# Layout v1 text specimen (import source)

Inline $E=mc^2$ and display math:

$$
\sum_{i=1}^n i
$$

```rust
fn main() {}
```

```mermaid
flowchart LR
    A[Author] --> B[.tes]
```

| Feature | Id |
| --- | --- |
| Text spans | text_spans |
| Block captions | caption |

A short paragraph after the table.

> Note: Markdown import does not yet map title/caption onto the text header.
> Use Tessprek `\block{title="…" caption="…"}` (see `fixtures/samples/block_captions.tes`)
> or the golden `layout_v1_text.tes` encoder for round-trips.
