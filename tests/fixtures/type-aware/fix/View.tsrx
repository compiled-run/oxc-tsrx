function identity<T = string>(value: T): T {
  return value;
}

export function View() @{
  const value = identity<string>("saved");
  <main>{value}</main>;
}
