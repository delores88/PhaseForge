import json,os,pathlib,subprocess,sys,unittest
import psutil
from process_probe import same_process

class ProcessIdentityTests(unittest.TestCase):
    def test_reused_pid_identity_is_never_treated_as_owned(self):
        process=psutil.Process(os.getpid())
        self.assertIsNone(same_process({'pid':process.pid,'created':process.create_time()-1}))
        self.assertEqual(same_process({'pid':process.pid,'created':process.create_time()}).pid,process.pid)

    def test_snapshot_requires_the_original_owner_creation_time(self):
        process=psutil.Process(os.getpid())
        command=[sys.executable,str(pathlib.Path(__file__).with_name('process_probe.py')),'snapshot','--pid',str(process.pid),'--created',str(process.create_time()-1)]
        self.assertEqual(json.loads(subprocess.check_output(command,text=True)),[])
        command[-1]=str(process.create_time())
        rows=json.loads(subprocess.check_output(command,text=True))
        self.assertTrue(any(row['pid']==process.pid and row['created']==process.create_time() for row in rows))

if __name__=='__main__':unittest.main()
