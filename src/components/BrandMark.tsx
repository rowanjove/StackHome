type BrandMarkProps = {
  className?: string;
  title?: string;
};

export function BrandMark({ className, title }: BrandMarkProps) {
  return (
    <svg
      className={className}
      viewBox="0 0 512 512"
      role={title ? "img" : undefined}
      aria-hidden={title ? undefined : true}
      aria-label={title}
      fill="none"
    >
      <path fill="currentColor" d="M72 306h368v112c0 24.301-19.699 44-44 44H116c-24.301 0-44-19.699-44-44V306Z" />
      <path className="brand-mark-paper" d="M124 338h264v48c0 13.255-10.745 24-24 24H148c-13.255 0-24-10.745-24-24v-48Z" />
      <path fill="currentColor" d="M116 122c0-13.255 10.745-24 24-24h148l76 76v148H116V122Z" />
      <path className="brand-mark-accent" d="M164 74c0-13.255 10.745-24 24-24h148l76 76v180H164V74Z" />
      <path className="brand-mark-paper" d="M204 112h104l64 64v130H204V112Z" />
      <path fill="currentColor" d="M308 112v64h64l-64-64Z" />
      <path className="brand-mark-accent" d="M236 216h104v18H236zM236 254h72v18h-72z" />
    </svg>
  );
}
