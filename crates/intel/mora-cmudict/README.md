# mora-cmudict

An offline English pronunciation provider for [`mora`](https://crates.io/crates/mora).
It keeps the 3.7 MB lexicon and its separate license outside Mora's
language-neutral, `no_std`, zero-dependency core.

The bundled `data/cmudict.dict` is the CMUSphinx dictionary distributed by
`cmudict-fast` 0.8.0. Its SHA-256 is
`59d6398f55297e59afb2ca3276380827524c0940fcbbfcd19022bb76fd55f719`.
The dictionary's redistribution terms are reproduced in `LICENSE-CMUDICT`.

`Cmudict::embedded()` parses the data once on first use and preserves alternate
pronunciations. Lookups are case-insensitive; unknown words return `None`.
