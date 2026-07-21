---
title: TSRX Syntax Support
description: The TSRX grammar slice the native overlay recognizes today, and how unsupported grammar fails.
---

# TSRX Syntax Support

The native scanner (`tsrx_syntax::scan`) reads your `.tsrx` file once, byte by
byte, and records where every piece of TSRX-only syntax starts and ends. That
record is called the overlay.

This page lists exactly which syntax the overlay recognizes today. Anything
outside this list **fails closed**: instead of guessing what your code means
and possibly producing wrong output, the command stops and reports an error
that tells you what it found and where.

## Control flow

### Statement containers

`@{ }` is a statement container, the successor and extension of JSX expression
containers: where JSX's `{ }` holds a single expression, `@{ }` holds ordinary
statements.

```tsrx
@{
  const doubled = count * 2;
}
```

### Conditionals

```tsrx
@if (user.isAdmin) {
  <AdminPanel />
} @else if (user.isMember) {
  <MemberPanel />
} @else {
  <SignIn />
}
```

### Loops

`@for` supports `for await`, declaration and assignment bindings, and
`index`/`key` annotations. `@empty` renders when the iterable produces
nothing:

```tsrx
@for (const item of items; index i; key item.id) {
  <Row item={item} position={i} />
} @empty {
  <EmptyState />
}
```

### Switch

```tsrx
@switch (status) {
  @case 'loading': { <Spinner /> }
  @case 'error': { <ErrorBanner /> }
  @default: { <Content /> }
}
```

### Try / pending / catch

Catch clauses support the headerless, error-binding, and
error-plus-reset-binding forms:

```tsrx
@try {
  <Profile user={await loadUser()} />
} @pending {
  <Skeleton />
} @catch (error; reset) {
  <Retry error={error} onRetry={reset} />
}
```

## Positions

Every control form is recognized in statement, direct JSX-child, nested, and
expression positions.

## Dynamic JSX tags

Matched dynamic opening/closing tags are recognized and validated against the
real expression AST from the single canonical OXC parse, with no lexical
approximation and no second parser:

```tsrx
<{condition ? Primary : Fallback} prop={value}>
  children
</{condition ? Primary : Fallback}>
```

Identities are structurally normalized (enclosing parentheses and trivia are
stripped; edge comments are retained), so equivalent opening and closing
expressions match. Dynamic tag expressions containing *nested dynamic JSX* are
not yet supported.

## Raw style elements

Lowercase raw `<style>` elements are recognized with opaque payload spans. The
CSS bytes are preserved exactly: carried through lint and format untouched,
never CSS-formatted or CSS-validated.

## Protected regions

The scanner protects strings, comments, regex literals, template text and
interpolation, JSX text and attributes, and multi-byte Unicode. An `@if`
inside a string literal is just text.

## Failing closed

The scanner rejects:

- orphan or reordered clauses (an `@else` without an `@if`, `@case` outside
  `@switch`);
- mismatched static or dynamic JSX closing tags;
- stale same-length overlays, by source fingerprint; and
- unsupported or incomplete grammar, including malformed editor states.

Failing closed is a deliberate contract: a fast path must not achieve its
numbers by silently dropping requested behavior.
