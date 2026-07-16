export function DynamicStyled({ tag, Other }: { tag: string; Other: string }) @{
  <section class="card">
    <{tag} data-kind="main">
      Hi <span>{tag}</span>
    </{tag}>
    <{Other} />
    <style>.card{color:red}.title{font-weight:700}</style>
    <h2 class="title">Done</h2>
  </section>;
}
