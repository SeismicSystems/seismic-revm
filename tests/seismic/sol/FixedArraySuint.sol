contract FixedArraySuint {
    uint256[10] arr;

    uint256 public x;
    uint256 internal y;
    uint256 private z;


    function test() public {
        for (uint256 i = 0; i < 10; i++) {
            arr[i] = 2 ** i;
        }

        suint256 index = 5;
        uint256 val = arr[index];
        assert(val == 32);
    }
}