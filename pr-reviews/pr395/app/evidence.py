from pathlib import Path
import subprocess,sys,json,time
BASE=Path('/private/tmp/claude-502/-Users-shiranui-dev-ReShiki/dd6a248f-7145-4e1e-bc5b-a50dfb88eed0/scratchpad')
ART=BASE/'ev386-2-final/app'
PATTERN='apps386-final-2/ReShiki386F2Evidence.app/Contents/MacOS/reshiki'
def owned(pid):
 p=subprocess.check_output(['pgrep','-f',PATTERN],text=True).split()
 assert p==[str(pid)],p
 return subprocess.check_output([str(BASE/'ev386-1b-wid'),str(pid)],text=True).strip()
if sys.argv[1]=='capture':
 pid=int(sys.argv[2]); name=sys.argv[3]; wid=owned(pid)
 subprocess.run(['screencapture','-x','-o','-l',wid,str(ART/name)],check=True)
 print(f'{name}: PID {pid}, WID {wid}')
elif sys.argv[1]=='drag':
 pid=int(sys.argv[2]); owned(pid)
 args=[str(BASE/'ev386-1b-drag')]+sys.argv[2:]
 with (ART/'drag-commands.txt').open('a') as f:f.write(' '.join(args)+'\n')
 subprocess.run(args,check=True)
elif sys.argv[1]=='cleanup':
 pid=int(sys.argv[2]); run=sys.argv[3]; owned(pid)
 subprocess.run(['kill',str(pid)],check=True)
 for _ in range(30):
  r=subprocess.run(['pgrep','-f',PATTERN],text=True,capture_output=True)
  if r.returncode==1:break
  time.sleep(.1)
 allp=subprocess.run(['pgrep','-lf','reshiki'],text=True,capture_output=True)
 out=f'RUN {run}; killed only PID {pid}\nVerification pgrep returncode={r.returncode}, stdout={r.stdout!r}\npgrep -lf reshiki:\n{allp.stdout}\n'
 with (ART/'process-cleanup.txt').open('a') as f:f.write(out)
 print(out)
 assert r.returncode==1
