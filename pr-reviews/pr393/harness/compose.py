import sys, pathlib
from PIL import Image, ImageDraw, ImageFont
root = pathlib.Path(sys.argv[1])
names = ["methoxide_000","methoxide_045","methoxide_090","methoxide_135","methoxide_200","methoxide_045_rot30"]
labels = ["drag 0°","drag 45°","drag 90°","drag 135°","drag 200°","45° panel, rotated 30°"]
cell = 420
font = ImageFont.load_default(size=18)
dims=[Image.open(root/s/f'{n}.png').size for s in ['before','after'] for n in names]
k=(cell-10)/max(max(w,h) for w,h in dims)
out = Image.new("RGB",(110+cell*len(names), 40+2*cell),"white")
d = ImageDraw.Draw(out)
for row, side in enumerate(["before","after"]):
    d.text((8, 40+row*cell+cell//2), "main" if side=="before" else "branch", fill="black", font=font)
    for i,n in enumerate(names):
        im = Image.open(root/side/f"{n}.png"); bg=Image.new("RGBA",im.size,"white"); bg.alpha_composite(im.convert("RGBA")); im=bg.convert("RGB"); im=im.resize((round(im.width*k),round(im.height*k)), Image.LANCZOS)
        x = 110+i*cell; y = 40+row*cell
        out.paste(im,(x+(cell-im.width)//2, y+(cell-im.height)//2))
        d.rectangle([x,y,x+cell-1,y+cell-1], outline="#ccc")
for i,l in enumerate(labels): d.text((110+i*cell+8,10), l, fill="black", font=font)
out.save(root/"comparison.png")
