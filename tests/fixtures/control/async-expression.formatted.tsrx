async function collect(items: AsyncIterable<{ id: string }>) {
  const rendered = @for await (const item of items; index i; key item.id) {
    <span>
      {i}:{item.id}
    </span>;
  } @empty {
    <em>Empty</em>;
  };
  return rendered;
}
