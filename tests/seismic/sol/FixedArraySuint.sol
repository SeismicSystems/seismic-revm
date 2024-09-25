contract FixedArraySuint {
    uint256[10] arr;

    function test() public {
        for (uint256 i = 0; i < 10; i++) {
            arr[i] = 2 ** i;
        }

        suint256 index = 5;
        uint256 val = arr[index];
        assert(val == 32);
    }
}