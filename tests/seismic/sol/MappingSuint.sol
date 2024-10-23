struct TestStruct {
    uint256 a;
    suint256 b;
}

contract MappingSuint {
    mapping(suint256 => suint256) suint_map;
    mapping(suint256 => TestStruct) suint_to_struct;
    mapping(suint256 => mapping(uint256 => uint256)) map_to_nested_uint256;
    mapping(suint256 => mapping(suint256 => uint256)) map_to_nested_suint256;

    function test() public {
        uint256 key = 10;
        uint256 val = 2;

        suint256 sKey = suint256(key);
        suint256 sVal = suint256(val);

        suint_map[sKey] = sVal;
        assert(suint_map[sKey] == val);
        assert(suint_map[0] == 0);

        TestStruct memory t = TestStruct(10, 15);
        suint_to_struct[sKey] = t;
        assert(suint_to_struct[sKey].a == 10);
        assert(suint_to_struct[sKey].b == 15);

        map_to_nested_uint256[sKey][5] = 20;
        assert(map_to_nested_uint256[sKey][5] == 20);
        assert(map_to_nested_uint256[sKey][0] == 0);
        assert(map_to_nested_uint256[0][5] == 0);

        suint256 sInnerKey = suint256(5);
        map_to_nested_suint256[sKey][sInnerKey] = 30;
        assert(map_to_nested_suint256[sKey][sInnerKey] == 30);
        assert(map_to_nested_suint256[sKey][0] == 0);
        assert(map_to_nested_suint256[0][sInnerKey] == 0);

        // Test non-existent keys
        assert(suint_map[suint256(100)] == 0);
        assert(suint_to_struct[suint256(100)].a == 0);
        assert(suint_to_struct[suint256(100)].b == 0);
        assert(map_to_nested_uint256[suint256(100)][100] == 0);
        assert(map_to_nested_suint256[suint256(100)][suint256(100)] == 0);
    }
}