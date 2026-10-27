"use client";

import { useEffect, useState, type ImgHTMLAttributes } from "react";
import { authenticatedImage } from "@/lib/api";

export default function AuthenticatedImage({ src, alt, ...props }: ImgHTMLAttributes<HTMLImageElement> & { src: string }) {
  const [image, setImage] = useState<{ path: string; url: string }>();
  const [error, setError] = useState<string>();
  useEffect(() => {
    const controller = new AbortController();
    let url: string | undefined;
    setError(undefined);
    authenticatedImage(src, controller.signal).then(blob => {
      if (controller.signal.aborted) return;
      url = URL.createObjectURL(blob);
      setImage({ path: src, url });
    }).catch(e => {
      if (!controller.signal.aborted) setError((e as Error).message);
    });
    return () => { controller.abort(); if (url) URL.revokeObjectURL(url); };
  }, [src]);
  if (error) return <span role="alert">影像載入失敗：{error}</span>;
  if (!image || image.path !== src) return <span role="status">載入影像…</span>;
  // eslint-disable-next-line @next/next/no-img-element
  return <img {...props} src={image.url} alt={alt} />;
}
