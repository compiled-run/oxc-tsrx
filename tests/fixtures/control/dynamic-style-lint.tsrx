export function DynamicLint({ tag }: { tag: string }) @{
  <{tag}
    class="card"
    onClick={() => {
      var legacy = 1;
      debugger;
      return legacy;
    }}
  >
    <style>
      .card { color: red; }
    </style>
    <span>{tag}</span>
  </{tag}>
}
