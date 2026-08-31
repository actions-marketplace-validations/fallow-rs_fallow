// Positive: a non-literal dangerouslySetInnerHTML value is a dangerous-html candidate.
export function Markup(props: { html: string }): JSX.Element {
  return <div dangerouslySetInnerHTML={{ __html: props.html }} />;
}
