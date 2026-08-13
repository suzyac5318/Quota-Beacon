export function createLatestRequestGate() {
  let current = 0;
  return {
    begin: () => ++current,
    isCurrent: (generation: number) => generation === current,
  };
}
