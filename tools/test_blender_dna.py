"""Pure parameter contracts; run without Blender. Actual geometry/render checked separately."""
import ast
import math
from pathlib import Path
import unittest

source = Path(__file__).with_name('blender_render.py')
tree = ast.parse(source.read_text(encoding='utf-8'))
namespace = {'math': math}
exec(compile(ast.Module(body=[n for n in tree.body if isinstance(n, ast.FunctionDef) and n.name == 'dna_parameters'], type_ignores=[]), str(source), 'exec'), namespace)
resolve = namespace['dna_parameters']

class DnaContract(unittest.TestCase):
    def test_original_failed_request_palette_count_and_radius_are_preserved(self):
        result = resolve({'radius':2.25, 'length':10, 'turns':2.6, 'count':24, 'thickness':.19, 'colors':['#2878D0','#E5B94C','#E95678','#43D6C5','#9B6DE3','#F08A3E']}, '#2F75D6')
        self.assertEqual(result['strand_colors'], ['#2878D0','#E5B94C'])
        self.assertEqual(result['base_colors'], ['#E95678','#43D6C5','#9B6DE3','#F08A3E'])
        self.assertEqual((result['count'], result['thickness'], result['length']), (24,.19,10))
        self.assertGreaterEqual(result['segments'], 2.6*128)

    def test_bounds_fail_instead_of_silently_changing_authored_geometry(self):
        for params in [{'count':24.5},{'count':True},{'count':0},{'count':161},{'turns':25},{'turns':float('nan')},{'radius':1e-10},{'thickness':1.1},{'colors':['blue','gold']},{'colors':['#123456']},{'colors':[]}]:
            with self.subTest(params=params), self.assertRaises(ValueError): resolve(params)

    def test_defaults_and_single_pair_are_explicit(self):
        default = resolve({}, '#123456')
        self.assertEqual(default['strand_colors'], ['#123456','#E5B94C'])
        self.assertEqual(default['count'],40)
        self.assertEqual(resolve({'turns':24})['count'],160)
        self.assertEqual(resolve({'count':1,'colors':['#123456','#abcdef']})['count'],1)
        self.assertEqual(len(default['base_colors']),4)

if __name__ == '__main__': unittest.main()
