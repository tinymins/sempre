/** Simple class name merger — joins truthy strings with a space. */
export const cn = (...classes: (string | false | null | undefined)[]) =>
  classes.filter(Boolean).join(" ");
