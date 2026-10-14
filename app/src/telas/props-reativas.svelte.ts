// Apoio de teste: objeto de props reativo ($state só funciona em .svelte.ts).
export function propsReativas<T extends object>(inicial: T): T {
  const props = $state(inicial);
  return props;
}
