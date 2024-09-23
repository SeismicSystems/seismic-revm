contract CastToUint {
    function test() public {
        suint256 x = 0x123456789;
        uint256 y = 0x123456789;
        uint256 z = uint256(x);

        assert(y == z);
    }
}