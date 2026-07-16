type Props = { ready: boolean };

export function View({ ready }: Props) {
  const contact = "@if@example.com";
  if (ready) {
    debugger;
    return <main data-contact={contact} />;
  }
  return <aside>idle</aside>;
}
