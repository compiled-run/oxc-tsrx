type Props = { items: string[] };

export function Ordinary({ items }: Props) {
  const banned = items.length;
  return <p>{banned}</p>;
}
