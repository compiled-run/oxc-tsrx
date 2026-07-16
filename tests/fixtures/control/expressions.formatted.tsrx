declare function consume(value: unknown): unknown;
export function choose(value: number) {
  const assigned = @switch (value) {
    @case 0: {
      <b>Zero</b>;
    }
    @default: {
      <i>Other</i>;
    }
  };
  consume(
    @try {
      <>{assigned}</>;
    } @catch (error) {
      <span>{String(error)}</span>;
    },
  );
  @try {
    <small>Side</small>;
  } @pending {
    <em>Wait</em>;
  };
  return @try {
    @switch (value) {
      @case 1: {
        <u>One</u>;
      }
      @default: {
        <u>Many</u>;
      }
    }
  } @catch {
    <strong>Failed</strong>;
  };
}
