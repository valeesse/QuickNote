export function pickImageFile(onSelect: (file: File) => void | Promise<void>): void {
  const input = document.createElement("input");
  input.type = "file";
  input.accept = "image/*";
  input.onchange = (event) => {
    const file = (event.target as HTMLInputElement).files?.[0];
    if (file) void onSelect(file);
  };
  input.click();
}
