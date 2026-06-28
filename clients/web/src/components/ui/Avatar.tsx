import * as AvatarPrimitive from '@radix-ui/react-avatar';
import type { ComponentPropsWithoutRef } from 'react';

/**
 * Image-with-fallback avatar, in the shadcn shape (Radix Avatar under the hood):
 * `<Avatar>` wraps an `<AvatarImage>` that only shows once it has loaded, and an
 * `<AvatarFallback>` that renders meanwhile / on error. Pass `loading="lazy"` to
 * the image; the art it points at is the server's cached WebP.
 */
export function Avatar({
  className = '',
  ...props
}: Readonly<ComponentPropsWithoutRef<typeof AvatarPrimitive.Root>>) {
  return (
    <AvatarPrimitive.Root
      className={`relative flex shrink-0 overflow-hidden ${className}`}
      {...props}
    />
  );
}

export function AvatarImage({
  className = '',
  ...props
}: Readonly<ComponentPropsWithoutRef<typeof AvatarPrimitive.Image>>) {
  return <AvatarPrimitive.Image className={`h-full w-full object-cover ${className}`} {...props} />;
}

export function AvatarFallback({
  className = '',
  ...props
}: Readonly<ComponentPropsWithoutRef<typeof AvatarPrimitive.Fallback>>) {
  return (
    <AvatarPrimitive.Fallback
      className={`relative flex h-full w-full items-center justify-center ${className}`}
      {...props}
    />
  );
}
