# alembic

Founding reservation for **Alembic**, the Mere platform's recall and workshop
port.

An alembic is the still that sits on the athanor's constant low heat: what
distils is what the furnace has been holding. This port is that pair for your
own work — the memory it accretes, and the bounded actors that run over it.

It splits in the [castellan](https://crates.io/crates/castellan) mold:

- **the embeddable half** (feature `recall`): the memory surface — three
  levels (short-term, long-term, engram), promotion and eviction, the engram
  browser, and lexical and embedding recall over a mere's traces. A host that
  wants memory and no agents takes this alone.
- **the authority half**: the workshop — agent identity and purpose, granted
  reads, writes, actions and watches, model and tool selection, run history,
  pending petitions, refusals and costs, pause, revoke, retry, and dissolve,
  with exact attribution into the target application's history. It lives with
  the resident, whose scheduler runs the jobs.

**Athanor was always an agent.** The distillation furnace is a bounded actor
under a grant, so the workshop generalizes the furnace rather than standing
beside it.

The boundaries are the point: not
[distillery](https://crates.io/crates/distillery) (the model works — it runs
models, alembic runs work), not the store (engrams and retention are
eidetic's), and not the grant algebra (that is servitor's, over personae's
identity).

The package is `mere-alembic` because crates.io `alembic` is the Linux
Foundation's VFX-format binding; the library keeps the product name.

Lives in the [mere](https://github.com/merely-made/mere) workspace at
`ports/alembic`. No implementation yet.

The distillation plan is wired to `fleece`: a page engram will supply its
extracted `Article` to Athanor instead of raw page bytes.

## License

MPL-2.0
